#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Viewport {
    pub cols: usize,
    pub rows: usize,
}

pub const VIEWPORTS: &[Viewport] = &[
    Viewport { cols: 80, rows: 24 },
    Viewport {
        cols: 120,
        rows: 12,
    },
    Viewport {
        cols: 120,
        rows: 40,
    },
    Viewport {
        cols: 200,
        rows: 50,
    },
];

pub const DOCS_VIEWPORT: Viewport = Viewport {
    cols: 120,
    rows: 40,
};

pub const CELL_ASPECT: f64 = 2.0;
pub const PT_PER_ROW: f64 = 11.0;

pub struct ConcealFixture {
    pub name: &'static str,
    pub overlays_name: &'static str,
    pub source: &'static str,
}

pub const CONCEAL_FIXTURES: &[ConcealFixture] = &[
    ConcealFixture {
        name: "conceal-fourier",
        overlays_name: "conceal-fourier-overlays",
        source: FOURIER_CELL,
    },
    ConcealFixture {
        name: "conceal-linear-algebra",
        overlays_name: "conceal-linear-algebra-overlays",
        source: LINEAR_ALGEBRA_CELL,
    },
];

const FOURIER_CELL: &str = r"# ## The Fourier transform
#
# The forward transform is $\hat{f}(\xi) = \int_{-\infty}^{\infty} f(x) \, e^{-2\pi i x \xi} \, dx$
# and the inverse is $f(x) = \int_{-\infty}^{\infty} \hat{f}(\xi) \, e^{2\pi i x \xi} \, d\xi$.
#
# Parseval's theorem states $\int |f(x)|^2 \, dx = \int |\hat{f}(\xi)|^2 \, d\xi$, so the
# transform preserves energy: $\|f\|_2 = \|\hat{f}\|_2$ for every $f \in L^2(\mathbb{R})$.
";

const LINEAR_ALGEBRA_CELL: &str = r"# ## The symmetric eigenproblem
#
# For $A \in \mathbb{R}^{n \times n}$ with $A^\top = A$ there is an orthogonal $Q$ such that
# $Q^\top A Q = \Lambda$ and $\Lambda = \mathrm{diag}(\lambda_1, \ldots, \lambda_n)$.
#
# Every eigenpair satisfies $A \mathbf{v}_i = \lambda_i \mathbf{v}_i$ with $\|\mathbf{v}_i\|_2 = 1$,
# the spectral norm is $\|A\|_2 = \max_i |\lambda_i|$, and $\mathrm{tr}(A) = \sum_i \lambda_i$.
";

pub struct ErrorFixture {
    pub name: &'static str,
    pub error_json: &'static str,
    pub raw_error: &'static str,
    pub notebook: Option<&'static str>,
}

pub const ERROR_FIXTURES: &[ErrorFixture] = &[
    ErrorFixture {
        name: "error-bounds",
        error_json: BOUNDS_ERROR_JSON,
        raw_error: "",
        notebook: None,
    },
    ErrorFixture {
        name: "error-undefined-symbol",
        error_json: UNDEFINED_SYMBOL_JSON,
        raw_error: "",
        notebook: Some(EIGEN_NOTEBOOK),
    },
    ErrorFixture {
        name: "error-undefined-variable",
        error_json: UNDEFINED_VARIABLE_JSON,
        raw_error: "",
        notebook: Some(EIGEN_NOTEBOOK),
    },
    ErrorFixture {
        name: "error-long-message",
        error_json: LONG_MESSAGE_JSON,
        raw_error: "",
        notebook: None,
    },
    ErrorFixture {
        name: "error-long-source-line",
        error_json: LONG_SOURCE_LINE_JSON,
        raw_error: "",
        notebook: None,
    },
];

pub const EIGEN_NOTEBOOK: &str = r"# ═══ Nothelix Notebook: linear-algebra.ipynb ═══
# Cells: 6

@cell 0 :julia # Imports
using LinearAlgebra
using Statistics

@markdown 1
# ## Symmetric eigenproblem
#
# Build a symmetric matrix, normalise it, then read off its spectrum.

@cell 2 :julia # Build A
n = 4
M = randn(n, n)
A = M + M'

@cell 3 :julia # Normalise
A = A / opnorm(A)

@cell 4 :julia # Sanity check
@assert A == A'

@cell 5 :julia # Spectrum
vals, vecs = eigen(A)
";

const BOUNDS_ERROR_JSON: &str = r#"{
  "error_type": "BoundsError",
  "message": "BoundsError: attempt to access 4-element Vector{Float64} at index [0]",
  "source_line": "    smallest = eigenvalues[0]",
  "cell_index": 7,
  "cell_line": 3,
  "frames": [
    {"file": "none", "line": 3, "func": "spectral_gap", "is_user_code": true},
    {"file": "/usr/share/julia/stdlib/v1.11/LinearAlgebra/src/symmetric.jl", "line": 611, "func": "eigen", "is_user_code": false},
    {"file": "/usr/share/julia/stdlib/v1.11/LinearAlgebra/src/eigen.jl", "line": 239, "func": "eigen!", "is_user_code": false},
    {"file": "none", "line": 8, "func": "top-level scope", "is_user_code": true}
  ]
}"#;

const UNDEFINED_SYMBOL_JSON: &str = r#"{
  "error_type": "UndefVarError",
  "message": "UndefVarError: `eigen` not defined in `Main`\nSuggestion: check for spelling errors or missing imports.\nHint: a global variable of this name also exists in LinearAlgebra.",
  "source_line": "vals, vecs = eigen(A)",
  "cell_index": 5,
  "cell_line": 1
}"#;

const UNDEFINED_VARIABLE_JSON: &str = r#"{
  "error_type": "UndefVarError",
  "message": "UndefVarError: `A` not defined in `Main`\nSuggestion: check for spelling errors or missing imports.",
  "source_line": "vals, vecs = eigen(A)",
  "cell_index": 5,
  "cell_line": 1
}"#;

const LONG_MESSAGE_JSON: &str = r#"{
  "error_type": "MethodError",
  "message": "MethodError: no method matching solve(::SparseMatrixCSC{Float64, Int64}, ::Vector{Float64}, ::Val{:cholesky}, ::NamedTuple{(:tol, :maxiter, :verbose), Tuple{Float64, Int64, Bool}})",
  "source_line": "    x = solve(K, f, Val(:cholesky), (tol = 1e-10, maxiter = 500, verbose = true))",
  "cell_index": 11,
  "cell_line": 4
}"#;

const LONG_SOURCE_LINE_JSON: &str = r#"{
  "error_type": "DimensionMismatch",
  "message": "DimensionMismatch: second dimension of A, 5, does not match length of x, 4",
  "source_line": "    A = [1.0 2.0 3.0 4.0 5.0; 6.0 7.0 8.0 9.0 10.0; 11.0 12.0 13.0 14.0 15.0; 16.0 17.0 18.0 19.0 20.0; 21.0 22.0 23.0 24.0 25.0] * x",
  "cell_index": 9,
  "cell_line": 2
}"#;

pub struct ReflowFixture {
    pub name: &'static str,
    pub source: &'static str,
}

pub const REFLOW_FIXTURES: &[ReflowFixture] = &[
    ReflowFixture {
        name: "math-reflow-cases",
        source: CASES_ENV_CELL,
    },
    ReflowFixture {
        name: "math-reflow-pmatrix",
        source: PMATRIX_ENV_CELL,
    },
];

const CASES_ENV_CELL: &str = r"# The ramp is defined piecewise.
# $$\begin{cases} x^2 & \text{if } x \geq 0 \\ -x^2 & \text{otherwise} \end{cases}$$
# It is continuously differentiable at the origin.
";

const PMATRIX_ENV_CELL: &str = r"# Rotating by $\theta$ in the plane.
# $$ R(\theta) = \begin{pmatrix} \cos\theta & -\sin\theta \\ \sin\theta & \cos\theta \end{pmatrix} $$
";

pub struct EquationSize {
    pub name: &'static str,
    pub width_pt: f64,
    pub height_pt: f64,
}

pub const EQUATION_SIZES: &[EquationSize] = &[
    EquationSize {
        name: "inline-identity",
        width_pt: 180.0,
        height_pt: 14.0,
    },
    EquationSize {
        name: "stacked-fraction",
        width_pt: 220.0,
        height_pt: 180.0,
    },
    EquationSize {
        name: "block-matrix",
        width_pt: 340.0,
        height_pt: 420.0,
    },
    EquationSize {
        name: "wide-alignment",
        width_pt: 900.0,
        height_pt: 40.0,
    },
];

pub const RESERVE_DOCUMENT: &str = r"@markdown 0
# ## Least squares
#
# The normal equations are
# $$ A^\top A \hat{x} = A^\top b $$
#
# which unroll into the stacked system
# $$
# \begin{pmatrix} a_{11} & a_{12} \\ a_{21} & a_{22} \end{pmatrix}
# \begin{pmatrix} x_1 \\ x_2 \end{pmatrix}
# =
# \begin{pmatrix} b_1 \\ b_2 \end{pmatrix}
# $$
#
# with residual
# $$ r = b - A \hat{x} $$
";

pub const RESERVE_BLOCK_SIZES: &[EquationSize] = &[
    EquationSize {
        name: "normal-equations",
        width_pt: 210.0,
        height_pt: 18.0,
    },
    EquationSize {
        name: "stacked-system",
        width_pt: 340.0,
        height_pt: 420.0,
    },
    EquationSize {
        name: "residual",
        width_pt: 160.0,
        height_pt: 180.0,
    },
];

pub const PLOT_SERIES_JSON: &str = r#"[
  {"x": [0.0, 0.5, 1.0, 1.5, 2.0, 2.5, 3.0, 3.5, 4.0, 4.5, 5.0, 5.5, 6.0, 6.5, 7.0, 7.5, 8.0, 8.5, 9.0, 9.5, 10.0],
   "y": [0.0, 0.454, 0.755, 0.798, 0.605, 0.269, -0.089, -0.351, -0.454, -0.394, -0.216, 0.0, 0.176, 0.259, 0.235, 0.129, -0.006, -0.124, -0.187, -0.183, -0.122],
   "label": "damped sine"},
  {"x": [0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0],
   "y": [-1.0, -0.8, -0.6, -0.4, -0.2, 0.0, 0.2, 0.4, 0.6, 0.8, 1.0],
   "label": "ramp"}
]"#;

pub const MARKDOWN_CELL: &str = r"# Spectral analysis

The **power spectrum** of a signal $s(t)$ is $|\hat{s}(\xi)|^2$, and its
integral is the total *energy*.

| window | leakage | main lobe |
|---|---|---|
| rectangular | high | narrow |
| hann | low | wide |

1. Window the signal.
2. Take the transform.
3. Square the magnitude.

See `DSP.welch_pgram` for the averaged estimator.
";
