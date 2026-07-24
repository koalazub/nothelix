mod call_chain;
mod enrichment;
mod hints;
mod matching;
mod path;
mod render;
mod scan;
mod text;
mod tokenize;
mod types;
#[cfg(feature = "native")]
mod undef;

use crate::error::{Result, ffi};

pub use types::{ErrorFrame, MethodCandidate, ScopeVarEntry, StructuredError, VarContext};

pub struct FormatContext<'a> {
    pub error_json: &'a str,
    pub raw_error: &'a str,
    pub notebook_path: Option<&'a str>,
}

pub fn format_error(ctx: &FormatContext<'_>) -> String {
    ffi(formatted(ctx))
}

fn formatted(ctx: &FormatContext<'_>) -> Result<String> {
    let hints = hints::hints()?;
    let kernel_dir = ctx
        .notebook_path
        .and_then(|p| std::path::Path::new(p).parent())
        .and_then(|d| d.to_str())
        .filter(|d| !d.is_empty())
        .map(str::to_owned);

    if let Ok(mut err) = serde_json::from_str::<StructuredError>(ctx.error_json)
        && !err.error_type.is_empty()
    {
        enrichment::apply(&mut err, ctx);
        return Ok(render::format_structured(
            &err,
            hints,
            kernel_dir.as_deref(),
        ));
    }

    if ctx.raw_error.is_empty() {
        return Ok("error: unknown\n".to_string());
    }
    Ok(render::format_raw(
        ctx.raw_error,
        hints,
        kernel_dir.as_deref(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw(input: &str) -> String {
        render::format_raw(
            input,
            hints::hints().expect("error_hints.toml must parse"),
            None,
        )
    }

    fn structured(error_json: &str) -> String {
        format_error(&FormatContext {
            error_json,
            raw_error: "",
            notebook_path: None,
        })
    }

    #[test]
    fn file_not_found_names_the_kernel_directory() {
        let json = r#"{
            "error_type": "SystemError",
            "message": "opening file \"spring_extract.wav\": No such file or directory",
            "frames": []
        }"#;
        let with_path = format_error(&FormatContext {
            error_json: json,
            raw_error: "",
            notebook_path: Some("/Users/someone/uni/sem_2/tutorial.jl"),
        });
        assert!(
            with_path.contains("= note: the kernel looked in /Users/someone/uni/sem_2"),
            "got:\n{with_path}"
        );
        let without_path = structured(json);
        assert!(
            !without_path.contains("the kernel looked in"),
            "got:\n{without_path}"
        );
    }

    #[test]
    fn bounds_error_zero() {
        let out = raw("BoundsError: attempt to access 3-element Vector{Int64} at index [0]");
        assert!(out.contains("E001"), "got:\n{out}");
        assert!(out.contains("1-indexed"));
    }

    #[test]
    fn undef_var() {
        let out = raw("UndefVarError: myvar not defined");
        assert!(out.contains("E004"), "got:\n{out}");
        assert!(out.contains("myvar"));
    }

    #[test]
    fn method_error_function_as_arg() {
        let out = raw("MethodError: no method matching /(::Int64, ::typeof(sqrt))");
        assert!(out.contains("E005"), "got:\n{out}");
        assert!(out.contains("sqrt"));
    }

    #[test]
    fn closest_candidates_stripped() {
        let out = raw(
            "MethodError: no method matching /(::Int64, ::typeof(sqrt))\nClosest candidates are:\n  lots of noise",
        );
        assert!(!out.contains("Closest candidates"));
    }

    #[test]
    fn missing_value_error() {
        let out = raw("MethodError: no method matching norm(::Base.Missing)");
        assert!(out.contains("E021"), "got:\n{out}");
        assert!(out.contains("missing"));
    }

    #[test]
    fn nothing_iterate_error() {
        let out = raw("MethodError: no method matching iterate(::Nothing)");
        assert!(out.contains("E024"), "got:\n{out}");
    }

    #[test]
    fn vector_plus_scalar() {
        let out = raw("MethodError: no method matching +(::Vector{Float64}, ::Int64)");
        assert!(out.contains("E060"), "got:\n{out}");
        assert!(out.contains(".+"));
    }

    #[test]
    fn vector_times_vector() {
        let out = raw("MethodError: no method matching *(::Vector{Float64}, ::Vector{Float64})");
        assert!(out.contains("E064"), "got:\n{out}");
        assert!(out.contains("dot"));
    }

    #[test]
    fn empty_collection() {
        let out = raw("ArgumentError: reducing over an empty collection is not allowed");
        assert!(out.contains("E071"), "got:\n{out}");
    }

    #[test]
    fn matrix_not_square() {
        let out = raw("ArgumentError: matrix is not square: dimensions are (2, 3)");
        assert!(out.contains("E072"), "got:\n{out}");
    }

    #[test]
    fn eigen_svd_order_mismatch() {
        let out = raw("AssertionError: eigvals(C) ≈ svdvals(X) .^ 2");
        assert!(out.contains("E100"), "got:\n{out}");
        assert!(out.contains("opposite orders"), "got:\n{out}");
    }

    #[test]
    fn plain_assertion_keeps_generic_hint() {
        let out = raw("AssertionError: x > 0");
        assert!(out.contains("E020"), "got:\n{out}");
    }

    #[test]
    fn objects_not_callable() {
        let out = raw("MethodError: objects of type Float64 are not callable");
        assert!(out.contains("E043"), "got:\n{out}");
    }

    #[test]
    fn unmatched_extracts_type() {
        let out = raw("CustomError: something new");
        assert!(out.contains("error[CustomError]"));
    }

    #[test]
    fn empty_input_reports_unknown() {
        let out = format_error(&FormatContext {
            error_json: "",
            raw_error: "",
            notebook_path: None,
        });
        assert_eq!(out, "error: unknown\n");
    }

    #[test]
    fn structured_with_call_chain() {
        let out = structured(
            r#"{
            "error_type": "MethodError",
            "message": "no method matching norm(::Base.Missing)",
            "frames": [
                {"file": "<cell>", "line": 5, "func": "compute_result", "is_user_code": true},
                {"file": "/nix/store/8h9qwxffgyisf9hiscw5ms6l56w6mni5-julia-bin-1.12.5/share/julia/stdlib/v1.12/LinearAlgebra/src/generic.jl", "line": 760, "func": "norm", "is_user_code": false}
            ],
            "source_line": "result = norm(data)",
            "cell_index": 3,
            "cell_line": 5
        }"#,
        );
        assert!(out.contains("E021"), "got:\n{out}");
        assert!(out.contains("^^^"));
        assert!(out.contains("stdlib:LinearAlgebra"));
        assert!(!out.contains("/nix/store/"));
    }

    #[test]
    fn cross_cell_context_unexecuted() {
        let out = structured(
            r#"{
            "error_type": "UndefVarError",
            "message": "`data` not defined",
            "frames": [],
            "source_line": "result = norm(data)",
            "cell_index": 5,
            "cell_line": 3,
            "cell_context": {
                "data": {
                    "source": "pending_registered",
                    "defined_in_cell": 2
                }
            }
        }"#,
        );
        assert!(
            out.contains("@cell 2"),
            "should reference defining cell, got:\n{out}"
        );
        assert!(
            out.contains("not yet executed"),
            "should say not executed, got:\n{out}"
        );
        assert!(
            out.contains("run @cell 2 first"),
            "should suggest running it, got:\n{out}"
        );
    }

    #[test]
    fn cross_cell_context_executed() {
        let out = structured(
            r#"{
            "error_type": "UndefVarError",
            "message": "`x` not defined",
            "frames": [],
            "source_line": "println(x)",
            "cell_index": 3,
            "cell_line": 1,
            "cell_context": {
                "x": {
                    "source": "executed",
                    "defined_in_cell": 1,
                    "status": "error"
                }
            }
        }"#,
        );
        assert!(out.contains("@cell 1"), "got:\n{out}");
        assert!(out.contains("status: error"), "got:\n{out}");
    }

    #[test]
    fn unexecuted_deps_rendered() {
        let out = structured(
            r#"{
            "error_type": "MethodError",
            "message": "no method matching foo(::Int64)",
            "frames": [],
            "source_line": "foo(1)",
            "cell_index": 4,
            "cell_line": 1,
            "unexecuted_deps": [1, 2]
        }"#,
        );
        assert!(out.contains("@cell 1"), "got:\n{out}");
        assert!(out.contains("@cell 2"), "got:\n{out}");
        assert!(out.contains("haven't been executed"), "got:\n{out}");
    }

    #[test]
    fn no_context_no_extra_output() {
        let out = structured(
            r#"{
            "error_type": "BoundsError",
            "message": "attempt to access 3-element Vector{Int64} at index [0]",
            "frames": [],
            "source_line": "v[0]",
            "cell_index": 1,
            "cell_line": 1
        }"#,
        );
        assert!(
            !out.contains("@cell"),
            "should have no cell context, got:\n{out}"
        );
        assert!(!out.contains("haven't been executed"), "got:\n{out}");
    }

    #[cfg(feature = "native")]
    #[test]
    fn undef_guidance_end_to_end_from_notebook() {
        use std::io::Write;
        let mut file = tempfile::Builder::new()
            .suffix(".jl")
            .tempfile()
            .expect("temp notebook");
        write!(
            file,
            "# ═══ Nothelix Notebook: nb.ipynb ═══\n# Cells: 3\n\n\
             @cell 3 :julia\nusing LinearAlgebra\n\n\
             @cell 65 :julia\nA = rand(3, 3)\n\n\
             @cell 70 :julia\neigen(A)\n"
        )
        .expect("write notebook");
        let path = file.path().to_string_lossy().into_owned();

        let out = format_error(&FormatContext {
            error_json: r#"{
            "error_type": "UndefVarError",
            "message": "UndefVarError: `eigen` not defined in `Main`. `A` not defined. a global variable of this name also exists in LinearAlgebra",
            "frames": [],
            "source_line": "eigen(A)",
            "cell_index": 70,
            "cell_line": 1
        }"#,
            raw_error: "",
            notebook_path: Some(&path),
        });

        assert!(
            out.contains("`eigen`, `A` are not defined"),
            "header should name both symbols, got:\n{out}"
        );
        assert!(
            out.contains("`eigen` comes from `LinearAlgebra`") && out.contains("@cell 3"),
            "package guidance missing, got:\n{out}"
        );
        assert!(
            out.contains("`A` is defined in @cell 65"),
            "user-binding guidance missing, got:\n{out}"
        );
        assert!(
            !out.contains("check spelling") && !out.contains("Check spelling"),
            "generic E004 help should be dropped, got:\n{out}"
        );
    }

    #[test]
    fn dimension_mismatch_uses_real_variable_names() {
        let out = structured(
            r#"{
            "error_type": "DimensionMismatch",
            "message": "a has size (8, 8), b has size (5, 5), mismatch at dim 1",
            "frames": [],
            "source_line": "result = S_hat * K",
            "cell_index": 3,
            "cell_line": 5
        }"#,
        );
        assert!(
            out.contains("S_hat"),
            "should use actual var name, got:\n{out}"
        );
        assert!(
            out.contains("(8, 8)"),
            "should show dimensions, got:\n{out}"
        );
        assert!(out.contains("K"), "should use actual var name, got:\n{out}");
        assert!(
            out.contains("(5, 5)"),
            "should show dimensions, got:\n{out}"
        );
        assert!(
            out.contains("size(S_hat, 2) == size(K, 1)"),
            "should show concrete check, got:\n{out}"
        );
    }

    #[test]
    fn dimension_mismatch_backslash_solve() {
        let out = structured(
            r#"{
            "error_type": "DimensionMismatch",
            "message": "a has size (3, 3), b has size (5,), mismatch at dim 1",
            "frames": [],
            "source_line": "x = A \\ b",
            "cell_index": 2,
            "cell_line": 1
        }"#,
        );
        assert!(out.contains("`A`"), "got:\n{out}");
        assert!(out.contains("`b`"), "got:\n{out}");
    }

    #[test]
    fn bounds_error_shows_actual_array_name() {
        let out = structured(
            r#"{
            "error_type": "BoundsError",
            "message": "attempt to access 5-element Vector{Float64} at index [10]",
            "frames": [],
            "source_line": "prices[10]",
            "cell_index": 1,
            "cell_line": 3
        }"#,
        );
        assert!(out.contains("prices"), "should name the array, got:\n{out}");
        assert!(out.contains("5 elements"), "should show count, got:\n{out}");
    }

    #[test]
    fn method_error_shows_actual_arg_types() {
        let out = structured(
            r#"{
            "error_type": "MethodError",
            "message": "no method matching similar(::Int64, ::Type{Float64})",
            "frames": [],
            "source_line": "B = similar(n, Float64)",
            "cell_index": 3,
            "cell_line": 5
        }"#,
        );
        assert!(
            out.contains("`n`"),
            "should show actual arg name, got:\n{out}"
        );
        assert!(out.contains("Int64"), "should show type, got:\n{out}");
        assert!(
            out.contains("typeof(n)"),
            "should suggest typeof check, got:\n{out}"
        );
    }

    #[test]
    fn method_error_type_constructor_parenthesized() {
        let out = structured(
            r#"{
            "error_type": "MethodError",
            "message": "no method matching (Matrix)(::Vector{ComplexF64})",
            "frames": [],
            "source_line": "C_inv = inv(Matrix(eigenvalues))",
            "cell_index": 19,
            "cell_line": 3
        }"#,
        );
        assert!(
            out.contains("`eigenvalues`"),
            "should show actual arg name, got:\n{out}"
        );
        assert!(
            out.contains("Vector{ComplexF64}"),
            "should show Vector type, got:\n{out}"
        );
        assert!(
            out.contains("typeof(eigenvalues)"),
            "should suggest typeof check, got:\n{out}"
        );
    }

    #[test]
    fn parse_error_reports_stray_close_bracket() {
        let out = structured(
            r##"{
            "error_type": "ParseError",
            "message": "# Error @ none:16:39\nExpected `end`",
            "frames": [],
            "source_line": "V = [sum(X(w - k*ωs) for k in -10:10)] for w in ω]",
            "cell_index": 25,
            "cell_line": 16
        }"##,
        );
        assert!(
            out.contains("bracket balance on this line"),
            "should name the balance issue, got:\n{out}"
        );
        assert!(
            out.contains("stray close") && out.contains("`]`"),
            "should identify the extra `]`, got:\n{out}"
        );
    }

    #[test]
    fn parse_error_reports_unclosed_bracket() {
        let out = structured(
            r##"{
            "error_type": "ParseError",
            "message": "# Error @ none:3:12\nExpected `end`",
            "frames": [],
            "source_line": "x = [1, 2, 3",
            "cell_index": 1,
            "cell_line": 3
        }"##,
        );
        assert!(
            out.contains("unclosed") && out.contains("`[`"),
            "should identify the unclosed `[`, got:\n{out}"
        );
    }

    #[test]
    fn parse_error_ignores_brackets_inside_strings() {
        let out = structured(
            r##"{
            "error_type": "ParseError",
            "message": "# Error @ none:1:12\nsomething",
            "frames": [],
            "source_line": "println(\"]\")",
            "cell_index": 0,
            "cell_line": 1
        }"##,
        );
        assert!(
            !out.contains("bracket balance"),
            "bracket inside string must not trigger imbalance note, got:\n{out}"
        );
    }

    #[test]
    fn not_callable_pinpoints_by_scope_type() {
        let out = structured(
            r##"{
            "error_type": "MethodError",
            "message": "MethodError: objects of type Vector{ComplexF64} are not callable\nUse square brackets [] for indexing an Array.",
            "frames": [],
            "source_line": "V = [sum(X(w - k*ωs) for k in -10:10) for w in ω]",
            "cell_index": 25,
            "cell_line": 8,
            "in_scope_variable_types": {
                "Vector{ComplexF64}": [{"name": "X", "cell": 17}]
            }
        }"##,
        );
        assert!(
            out.contains("called something of type Vector{ComplexF64}"),
            "got:\n{out}"
        );
        assert!(
            out.contains("`X` — Vector{ComplexF64} assigned in cell 17"),
            "should name X and cell 17, got:\n{out}"
        );
    }

    #[test]
    fn not_callable_without_scope_map_lists_call_names() {
        let out = structured(
            r##"{
            "error_type": "MethodError",
            "message": "MethodError: objects of type Vector{Int64} are not callable",
            "frames": [],
            "source_line": "result = foo(x) + bar(y)",
            "cell_index": 3,
            "cell_line": 2
        }"##,
        );
        assert!(
            out.contains("names called in the source line"),
            "got:\n{out}"
        );
        assert!(
            out.contains("`foo()`") && out.contains("`bar()`"),
            "got:\n{out}"
        );
    }

    #[test]
    fn method_error_with_kernel_scope_hints() {
        let out = structured(
            r#"{
            "error_type": "MethodError",
            "message": "no method matching (Matrix)(::Vector{ComplexF64})",
            "frames": [],
            "source_line": "C_inv = inv(Matrix(eigenvalues))",
            "cell_index": 19,
            "cell_line": 3,
            "in_scope_variable_types": {
                "Vector{ComplexF64}": [{"name": "eigenvalues", "cell": 17}],
                "Circulant{Int64, Vector{Int64}}": [{"name": "C1", "cell": 17}]
            },
            "method_candidates": [
                {"name": "C1", "type": "Circulant{Int64, Vector{Int64}}", "cell": 17}
            ]
        }"#,
        );
        assert!(
            out.contains("in-scope variables by type"),
            "should include scope block, got:\n{out}"
        );
        assert!(
            out.contains("Vector{ComplexF64}: `eigenvalues` (cell 17)"),
            "should map type → var, got:\n{out}"
        );
        assert!(
            out.contains("`Matrix()` accepts these in-scope values"),
            "should include candidates block, got:\n{out}"
        );
        assert!(
            out.contains("`C1`"),
            "should name C1 as candidate, got:\n{out}"
        );
    }

    #[test]
    fn method_error_without_kernel_hints_still_works() {
        let out = structured(
            r#"{
            "error_type": "MethodError",
            "message": "no method matching (Matrix)(::Vector{ComplexF64})",
            "frames": [],
            "source_line": "C_inv = inv(Matrix(eigenvalues))",
            "cell_index": 19,
            "cell_line": 3
        }"#,
        );
        assert!(out.contains("`eigenvalues`"), "got:\n{out}");
        assert!(
            !out.contains("in-scope variables by type"),
            "should not render empty scope block, got:\n{out}"
        );
    }

    #[test]
    fn method_error_multi_arg_enrichment() {
        let out = structured(
            r#"{
            "error_type": "MethodError",
            "message": "no method matching foo(::Vector{Float64}, ::String)",
            "frames": [],
            "source_line": "result = foo(data, label)",
            "cell_index": 1,
            "cell_line": 2
        }"#,
        );
        assert!(out.contains("`data`"), "got:\n{out}");
        assert!(out.contains("`label`"), "got:\n{out}");
        assert!(out.contains("typeof(data)"), "got:\n{out}");
        assert!(out.contains("typeof(label)"), "got:\n{out}");
    }
}
