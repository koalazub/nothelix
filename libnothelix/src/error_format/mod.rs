//! Ergonomic error formatting for Julia cell execution.
//!
//! Transforms raw Julia errors into Rust-compiler-style guided messages
//! with source context, call chains, and fix examples.
//!
//! Error matching uses token-based classification (no regex). An error
//! message is scanned into `ErrorTokens` (error type, function name,
//! type arguments, keywords), then each hint declares required/excluded
//! tokens for matching. Highest-specificity hint wins.

use std::sync::OnceLock;

mod enrichment;
mod hints;
mod matching;
mod render;
mod scanners;
mod tokenize;
mod types;
mod util;

use hints::hints;
use render::{format_raw, format_structured};
pub use types::{ErrorFrame, MethodCandidate, ScopeVarEntry, StructuredError, VarContext};
#[cfg(test)]
use util::clean_path;

// ─── Public entry point ──────────────────────────────────────────────────────

/// Format a Julia error into a guided message.
/// Inputs for a single error-formatting pass. All optional context that
/// enrichers might consume lives here — adding new enrichers means
/// threading a new field, not forking a new entry point.
pub struct FormatContext<'a> {
    pub error_json: &'a str,
    pub raw_error: &'a str,
    pub notebook_path: Option<&'a str>,
}

/// An enricher inspects the parsed error + the surrounding context and
/// may mutate the error in place (e.g. adding `cell_context` entries
/// discovered via notebook static scan, annotating frames, …).
trait Enricher: Send + Sync {
    fn enrich(&self, err: &mut StructuredError, ctx: &FormatContext<'_>);
}

fn enrichers() -> &'static [Box<dyn Enricher + Send + Sync>] {
    static ENRICHERS: OnceLock<Vec<Box<dyn Enricher + Send + Sync>>> = OnceLock::new();
    ENRICHERS
        .get_or_init(|| -> Vec<Box<dyn Enricher + Send + Sync>> {
            #[cfg(feature = "native")]
            {
                vec![Box::new(StaticCellScanEnricher)]
            }
            #[cfg(not(feature = "native"))]
            {
                Vec::new()
            }
        })
        .as_slice()
}

/// Unified formatter: deserialize the structured payload if present,
/// run every registered enricher against it, then format; otherwise
/// fall back to the raw-error path. Replaces the old pair of
/// `format_error` / `format_error_with_notebook` fork.
pub fn format_error(ctx: &FormatContext<'_>) -> String {
    let hints = hints();

    if let Ok(mut err) = serde_json::from_str::<StructuredError>(ctx.error_json)
        && !err.error_type.is_empty()
    {
        for enricher in enrichers() {
            enricher.enrich(&mut err, ctx);
        }
        return format_structured(&err, hints);
    }

    if !ctx.raw_error.is_empty() {
        return format_raw(ctx.raw_error, hints);
    }

    "error: unknown\n".to_string()
}

/// Populates `cell_context` for `UndefVarError` by scanning the notebook
/// `.jl` source for an assignment to the missing variable. Only fires
/// when the kernel supplied no context of its own.
#[cfg(feature = "native")]
struct StaticCellScanEnricher;

#[cfg(feature = "native")]
impl Enricher for StaticCellScanEnricher {
    fn enrich(&self, err: &mut StructuredError, ctx: &FormatContext<'_>) {
        if err.error_type != "UndefVarError" || !err.cell_context.is_empty() {
            return;
        }
        let Some(path) = ctx.notebook_path else {
            return;
        };
        if path.is_empty() {
            return;
        }
        let var = extract_undef_var(&err.message);
        if var.is_empty() {
            return;
        }

        let json = crate::notebook::scan_variable_definition(path.to_string(), var.clone());
        if json == "null" {
            return;
        }

        #[derive(serde::Deserialize)]
        struct Scanned {
            cell_index: i64,
            #[serde(default)]
            line_in_cell: i64,
            #[serde(default)]
            line_text: String,
        }
        if let Ok(hit) = serde_json::from_str::<Scanned>(&json) {
            if hit.cell_index == err.cell_index {
                return;
            }
            err.cell_context.insert(
                var,
                VarContext::StaticSource {
                    defined_in_cell: hit.cell_index,
                    line_in_cell: hit.line_in_cell,
                    line_text: hit.line_text,
                },
            );
        }
    }
}

#[cfg(feature = "native")]
fn extract_undef_var(msg: &str) -> String {
    // Prefer a backticked identifier.
    if let Some(start) = msg.find('`')
        && let Some(end) = msg[start + 1..].find('`')
    {
        let cand = &msg[start + 1..start + 1 + end];
        if scanners::is_identifier(cand) {
            return cand.to_string();
        }
    }
    // Fallback: first whitespace-delimited word that looks like an ident.
    for word in msg.split_whitespace() {
        if scanners::is_identifier(word) {
            return word.to_string();
        }
    }
    String::new()
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // Tokenizer white-box tests moved into tokenize.rs alongside their target.

    // ── Hint matching ──

    #[test]
    fn hints_load() {
        let hints = hints();
        assert!(hints.len() >= 50, "got {}", hints.len());
    }

    #[test]
    fn hints_cached_identity() {
        let a = hints();
        let b = hints();
        assert!(std::ptr::eq(a, b));
    }

    #[test]
    fn bounds_error_zero() {
        let raw = "BoundsError: attempt to access 3-element Vector{Int64} at index [0]";
        let hints = hints();
        let out = format_raw(raw, hints);
        assert!(out.contains("E001"), "got:\n{}", out);
        assert!(out.contains("1-indexed"));
    }

    #[test]
    fn undef_var() {
        let raw = "UndefVarError: myvar not defined";
        let hints = hints();
        let out = format_raw(raw, hints);
        assert!(out.contains("E004"), "got:\n{}", out);
        assert!(out.contains("myvar"));
    }

    #[test]
    fn method_error_function_as_arg() {
        let raw = "MethodError: no method matching /(::Int64, ::typeof(sqrt))";
        let hints = hints();
        let out = format_raw(raw, hints);
        assert!(out.contains("E005"), "got:\n{}", out);
        assert!(out.contains("sqrt"));
    }

    #[test]
    fn closest_candidates_stripped() {
        let raw = "MethodError: no method matching /(::Int64, ::typeof(sqrt))\nClosest candidates are:\n  lots of noise";
        let hints = hints();
        let out = format_raw(raw, hints);
        assert!(!out.contains("Closest candidates"));
    }

    #[test]
    fn missing_value_error() {
        let raw = "MethodError: no method matching norm(::Base.Missing)";
        let hints = hints();
        let out = format_raw(raw, hints);
        assert!(out.contains("E021"), "got:\n{}", out);
        assert!(out.contains("missing"));
    }

    #[test]
    fn nothing_iterate_error() {
        let raw = "MethodError: no method matching iterate(::Nothing)";
        let hints = hints();
        let out = format_raw(raw, hints);
        assert!(out.contains("E024"), "got:\n{}", out);
    }

    #[test]
    fn vector_plus_scalar() {
        let raw = "MethodError: no method matching +(::Vector{Float64}, ::Int64)";
        let hints = hints();
        let out = format_raw(raw, hints);
        assert!(out.contains("E060"), "got:\n{}", out);
        assert!(out.contains(".+"));
    }

    #[test]
    fn vector_times_vector() {
        let raw = "MethodError: no method matching *(::Vector{Float64}, ::Vector{Float64})";
        let hints = hints();
        let out = format_raw(raw, hints);
        assert!(out.contains("E064"), "got:\n{}", out);
        assert!(out.contains("dot"));
    }

    #[test]
    fn empty_collection() {
        let raw = "ArgumentError: reducing over an empty collection is not allowed";
        let hints = hints();
        let out = format_raw(raw, hints);
        assert!(out.contains("E071"), "got:\n{}", out);
    }

    #[test]
    fn matrix_not_square() {
        let raw = "ArgumentError: matrix is not square: dimensions are (2, 3)";
        let hints = hints();
        let out = format_raw(raw, hints);
        assert!(out.contains("E072"), "got:\n{}", out);
    }

    #[test]
    fn eigen_svd_order_mismatch() {
        let raw = "AssertionError: eigvals(C) ≈ svdvals(X) .^ 2";
        let hints = hints();
        let out = format_raw(raw, hints);
        assert!(out.contains("E100"), "got:\n{}", out);
        assert!(out.contains("opposite orders"), "got:\n{}", out);
    }

    #[test]
    fn plain_assertion_keeps_generic_hint() {
        let raw = "AssertionError: x > 0";
        let hints = hints();
        let out = format_raw(raw, hints);
        assert!(out.contains("E020"), "got:\n{}", out);
    }

    #[test]
    fn objects_not_callable() {
        let raw = "MethodError: objects of type Float64 are not callable";
        let hints = hints();
        let out = format_raw(raw, hints);
        assert!(out.contains("E043"), "got:\n{}", out);
    }

    #[test]
    fn structured_with_call_chain() {
        let json = r#"{
            "error_type": "MethodError",
            "message": "no method matching norm(::Base.Missing)",
            "frames": [
                {"file": "<cell>", "line": 5, "func": "compute_result", "is_user_code": true},
                {"file": "/nix/store/8h9qwxffgyisf9hiscw5ms6l56w6mni5-julia-bin-1.12.5/share/julia/stdlib/v1.12/LinearAlgebra/src/generic.jl", "line": 760, "func": "norm", "is_user_code": false}
            ],
            "source_line": "result = norm(data)",
            "cell_index": 3,
            "cell_line": 5
        }"#;
        let out = format_error(&FormatContext {
            error_json: json,
            raw_error: "",
            notebook_path: None,
        });
        assert!(out.contains("E021"), "got:\n{}", out);
        assert!(out.contains("^^^"));
        assert!(out.contains("stdlib:LinearAlgebra"));
        assert!(!out.contains("/nix/store/"));
    }

    #[test]
    fn unmatched_extracts_type() {
        let raw = "CustomError: something new";
        let hints = hints();
        let out = format_raw(raw, hints);
        assert!(out.contains("error[CustomError]"));
    }

    #[test]
    fn clean_nix_stdlib_path() {
        let p = "/nix/store/8h9qwxffgyisf9hiscw5ms6l56w6mni5-julia-bin-1.12.5/share/julia/stdlib/v1.12/LinearAlgebra/src/generic.jl";
        assert_eq!(clean_path(p), "stdlib:LinearAlgebra/src/generic.jl");
    }

    #[test]
    fn clean_julia_packages_path() {
        let p = "/home/user/.julia/packages/DataFrames/AbCdE/src/dataframe.jl";
        assert_eq!(clean_path(p), "DataFrames/src/dataframe.jl");
    }

    #[test]
    fn cross_cell_context_unexecuted() {
        let json = r#"{
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
        }"#;
        let out = format_error(&FormatContext {
            error_json: json,
            raw_error: "",
            notebook_path: None,
        });
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
        let json = r#"{
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
        }"#;
        let out = format_error(&FormatContext {
            error_json: json,
            raw_error: "",
            notebook_path: None,
        });
        assert!(out.contains("@cell 1"), "got:\n{out}");
        assert!(out.contains("status: error"), "got:\n{out}");
    }

    #[test]
    fn unexecuted_deps_rendered() {
        let json = r#"{
            "error_type": "MethodError",
            "message": "no method matching foo(::Int64)",
            "frames": [],
            "source_line": "foo(1)",
            "cell_index": 4,
            "cell_line": 1,
            "unexecuted_deps": [1, 2]
        }"#;
        let out = format_error(&FormatContext {
            error_json: json,
            raw_error: "",
            notebook_path: None,
        });
        assert!(out.contains("@cell 1"), "got:\n{out}");
        assert!(out.contains("@cell 2"), "got:\n{out}");
        assert!(out.contains("haven't been executed"), "got:\n{out}");
    }

    #[test]
    fn no_context_no_extra_output() {
        let json = r#"{
            "error_type": "BoundsError",
            "message": "attempt to access 3-element Vector{Int64} at index [0]",
            "frames": [],
            "source_line": "v[0]",
            "cell_index": 1,
            "cell_line": 1
        }"#;
        let out = format_error(&FormatContext {
            error_json: json,
            raw_error: "",
            notebook_path: None,
        });
        assert!(
            !out.contains("@cell"),
            "should have no cell context, got:\n{out}"
        );
        assert!(!out.contains("haven't been executed"), "got:\n{out}");
    }

    // ── Source context enrichment ──

    #[test]
    fn dimension_mismatch_uses_real_variable_names() {
        let json = r#"{
            "error_type": "DimensionMismatch",
            "message": "a has size (8, 8), b has size (5, 5), mismatch at dim 1",
            "frames": [],
            "source_line": "result = S_hat * K",
            "cell_index": 3,
            "cell_line": 5
        }"#;
        let out = format_error(&FormatContext {
            error_json: json,
            raw_error: "",
            notebook_path: None,
        });
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
        let json = r#"{
            "error_type": "DimensionMismatch",
            "message": "a has size (3, 3), b has size (5,), mismatch at dim 1",
            "frames": [],
            "source_line": "x = A \\ b",
            "cell_index": 2,
            "cell_line": 1
        }"#;
        let out = format_error(&FormatContext {
            error_json: json,
            raw_error: "",
            notebook_path: None,
        });
        assert!(out.contains("`A`"), "got:\n{out}");
        assert!(out.contains("`b`"), "got:\n{out}");
    }

    #[test]
    fn bounds_error_shows_actual_array_name() {
        let json = r#"{
            "error_type": "BoundsError",
            "message": "attempt to access 5-element Vector{Float64} at index [10]",
            "frames": [],
            "source_line": "prices[10]",
            "cell_index": 1,
            "cell_line": 3
        }"#;
        let out = format_error(&FormatContext {
            error_json: json,
            raw_error: "",
            notebook_path: None,
        });
        assert!(out.contains("prices"), "should name the array, got:\n{out}");
        assert!(out.contains("5 elements"), "should show count, got:\n{out}");
    }

    // The white-box tests for scan_dimension_pairs / scan_binary_operands
    // moved into error_format/enrichment/dimension.rs alongside their
    // targets. The integration cover via method_error_shows_actual_arg_types
    // below + dimension's own tests preserve the behavioural coverage.

    #[test]
    fn method_error_shows_actual_arg_types() {
        let json = r#"{
            "error_type": "MethodError",
            "message": "no method matching similar(::Int64, ::Type{Float64})",
            "frames": [],
            "source_line": "B = similar(n, Float64)",
            "cell_index": 3,
            "cell_line": 5
        }"#;
        let out = format_error(&FormatContext {
            error_json: json,
            raw_error: "",
            notebook_path: None,
        });
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
        // Julia writes type-constructor MethodErrors as `matching (Name)(...)`.
        // Previously enrich_method_error read the empty slice before the
        // first `(` as the name and bailed, producing `no method `` for these
        // argument types` — useless.
        let json = r#"{
            "error_type": "MethodError",
            "message": "no method matching (Matrix)(::Vector{ComplexF64})",
            "frames": [],
            "source_line": "C_inv = inv(Matrix(eigenvalues))",
            "cell_index": 19,
            "cell_line": 3
        }"#;
        let out = format_error(&FormatContext {
            error_json: json,
            raw_error: "",
            notebook_path: None,
        });
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

    // scan_types_from_call white-box test moved into
    // error_format/enrichment/method_error.rs alongside its target.

    #[test]
    fn parse_error_reports_stray_close_bracket() {
        // Real user case: `V = [... for w in ω]` had an extra `]`
        // mid-expression. Julia reports "Expected `end`" because its
        // parser got confused after the first stray close — not
        // useful. The enricher should call out the bracket imbalance
        // directly.
        let json = r##"{
            "error_type": "ParseError",
            "message": "# Error @ none:16:39\nExpected `end`",
            "frames": [],
            "source_line": "V = [sum(X(w - k*ωs) for k in -10:10)] for w in ω]",
            "cell_index": 25,
            "cell_line": 16
        }"##;
        let out = format_error(&FormatContext {
            error_json: json,
            raw_error: "",
            notebook_path: None,
        });
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
        let json = r##"{
            "error_type": "ParseError",
            "message": "# Error @ none:3:12\nExpected `end`",
            "frames": [],
            "source_line": "x = [1, 2, 3",
            "cell_index": 1,
            "cell_line": 3
        }"##;
        let out = format_error(&FormatContext {
            error_json: json,
            raw_error: "",
            notebook_path: None,
        });
        assert!(
            out.contains("unclosed") && out.contains("`[`"),
            "should identify the unclosed `[`, got:\n{out}"
        );
    }

    #[test]
    fn parse_error_ignores_brackets_inside_strings() {
        // `println("]")` is balanced — the `]` is inside a string literal.
        let json = r##"{
            "error_type": "ParseError",
            "message": "# Error @ none:1:12\nsomething",
            "frames": [],
            "source_line": "println(\"]\")",
            "cell_index": 0,
            "cell_line": 1
        }"##;
        let out = format_error(&FormatContext {
            error_json: json,
            raw_error: "",
            notebook_path: None,
        });
        assert!(
            !out.contains("bracket balance"),
            "bracket inside string must not trigger imbalance note, got:\n{out}"
        );
    }

    #[test]
    fn not_callable_pinpoints_by_scope_type() {
        // User's case: `V = [sum(X(w - k*ωₛ) for k in -10:10) for w in ω]`
        // where `X` was previously assigned a Vector{ComplexF64}. Julia
        // says "objects of type Vector{ComplexF64} are not callable".
        // With kernel's scope map we match the type to the called name
        // and tell the user exactly which cell defined it.
        let json = r##"{
            "error_type": "MethodError",
            "message": "MethodError: objects of type Vector{ComplexF64} are not callable\nUse square brackets [] for indexing an Array.",
            "frames": [],
            "source_line": "V = [sum(X(w - k*ωs) for k in -10:10) for w in ω]",
            "cell_index": 25,
            "cell_line": 8,
            "in_scope_variable_types": {
                "Vector{ComplexF64}": [{"name": "X", "cell": 17}]
            }
        }"##;
        let out = format_error(&FormatContext {
            error_json: json,
            raw_error: "",
            notebook_path: None,
        });
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
        // When the kernel hasn't been running, fall back to naming the
        // identifiers called on the offending line so the user knows
        // where to look.
        let json = r##"{
            "error_type": "MethodError",
            "message": "MethodError: objects of type Vector{Int64} are not callable",
            "frames": [],
            "source_line": "result = foo(x) + bar(y)",
            "cell_index": 3,
            "cell_line": 2
        }"##;
        let out = format_error(&FormatContext {
            error_json: json,
            raw_error: "",
            notebook_path: None,
        });
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
        // Kernel attached in_scope_variable_types + method_candidates:
        // Vector{ComplexF64} is held by `eigenvalues` (cell 17); Circulant
        // is held by `C1` (cell 17) and `Matrix(::Circulant)` has a method.
        // The enricher should render BOTH the "variables by type" block
        // and the "candidates" block using those values.
        let json = r#"{
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
        }"#;
        let out = format_error(&FormatContext {
            error_json: json,
            raw_error: "",
            notebook_path: None,
        });
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
        // No in_scope_variable_types / method_candidates → parenthesized
        // name parse + arg-type note should still render. Guarantees the
        // enricher degrades gracefully when the kernel isn't running.
        let json = r#"{
            "error_type": "MethodError",
            "message": "no method matching (Matrix)(::Vector{ComplexF64})",
            "frames": [],
            "source_line": "C_inv = inv(Matrix(eigenvalues))",
            "cell_index": 19,
            "cell_line": 3
        }"#;
        let out = format_error(&FormatContext {
            error_json: json,
            raw_error: "",
            notebook_path: None,
        });
        assert!(out.contains("`eigenvalues`"), "got:\n{out}");
        assert!(
            !out.contains("in-scope variables by type"),
            "should not render empty scope block, got:\n{out}"
        );
    }

    #[test]
    fn method_error_multi_arg_enrichment() {
        let json = r#"{
            "error_type": "MethodError",
            "message": "no method matching foo(::Vector{Float64}, ::String)",
            "frames": [],
            "source_line": "result = foo(data, label)",
            "cell_index": 1,
            "cell_line": 2
        }"#;
        let out = format_error(&FormatContext {
            error_json: json,
            raw_error: "",
            notebook_path: None,
        });
        assert!(out.contains("`data`"), "got:\n{out}");
        assert!(out.contains("`label`"), "got:\n{out}");
        assert!(out.contains("typeof(data)"), "got:\n{out}");
        assert!(out.contains("typeof(label)"), "got:\n{out}");
    }
}
