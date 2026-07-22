pub const CORPUS: &[(&str, &str)] = &[
    ("euler_identity", "e^(i pi) + 1 = 0"),
    ("golden_ratio", "\\varphi = \\frac{1 + \\sqrt{5}}{2}"),
    (
        "gamma_function",
        "\\Gamma(z) = \\int_0^\\infty t^{z-1} e^{-t} \\, dt",
    ),
    (
        "gamma_integral",
        "\\Gamma(n+1) = \\int_0^\\infty x^n e^{-x} \\, dx = n!",
    ),
    (
        "fourier_transform",
        "\\hat{f}(\\xi) = \\int_{-\\infty}^{\\infty} f(x) \\, e^{-2\\pi i x \\xi} \\, dx",
    ),
    (
        "inverse_fourier_transform",
        "f(x) = \\int_{-\\infty}^{\\infty} \\hat{f}(\\xi) \\, e^{2\\pi i x \\xi} \\, d\\xi",
    ),
    (
        "fourier_series",
        "f(x) \\sim \\sum_{n=-\\infty}^{\\infty} c_n \\, e^{i n \\pi x / L}",
    ),
    ("taylor_exp", "e^x = \\sum_{n=0}^{\\infty} \\frac{x^n}{n!}"),
    (
        "taylor_sin",
        "\\sin(x) = \\sum_{n=0}^{\\infty} (-1)^n \\frac{x^{2n+1}}{(2n+1)!}",
    ),
    (
        "taylor_general",
        "f(x) = f(a) + f'(a)(x-a) + \\frac{f''(a)}{2!}(x-a)^2 + \\cdots",
    ),
    (
        "binomial_series",
        "(1+x)^\\alpha = \\sum_{n=0}^{\\infty} \\binom{\\alpha}{n} x^n",
    ),
    (
        "det_2x2",
        "\\det(A) = \\begin{vmatrix} a & b \\\\ c & d \\end{vmatrix} = ad - bc",
    ),
    (
        "det_3x3",
        "\\begin{vmatrix} a & b & c \\\\ d & e & f \\\\ g & h & i \\end{vmatrix}",
    ),
    ("characteristic_poly", "\\det(A - \\lambda I) = 0"),
    (
        "matrix_3x3",
        "A = \\begin{pmatrix} 1 & 2 & 3 \\\\ 4 & 5 & 6 \\\\ 7 & 8 & 9 \\end{pmatrix}",
    ),
    (
        "matrix_inverse",
        "A^{-1} = \\frac{1}{\\det A} \\begin{pmatrix} d & -b \\\\ -c & a \\end{pmatrix}",
    ),
    (
        "matrix_bmatrix",
        "B = \\begin{bmatrix} \\lambda & 0 \\\\ 0 & \\lambda^{-1} \\end{bmatrix}",
    ),
    (
        "derivative_limit",
        "\\frac{d}{dx} f(x) = \\lim_{h \\to 0} \\frac{f(x+h) - f(x)}{h}",
    ),
    ("fundamental_theorem", "\\int_a^b f(x) \\, dx = F(b) - F(a)"),
    (
        "divergence",
        "\\nabla \\cdot \\mathbf{F} = \\frac{\\partial P}{\\partial x} + \\frac{\\partial Q}{\\partial y} + \\frac{\\partial R}{\\partial z}",
    ),
    (
        "curl",
        "\\nabla \\times \\mathbf{F} = \\begin{vmatrix} \\hat{i} & \\hat{j} & \\hat{k} \\\\ \\partial_x & \\partial_y & \\partial_z \\\\ P & Q & R \\end{vmatrix}",
    ),
    (
        "laplacian",
        "\\nabla^2 \\phi = \\frac{\\partial^2 \\phi}{\\partial x^2} + \\frac{\\partial^2 \\phi}{\\partial y^2} + \\frac{\\partial^2 \\phi}{\\partial z^2}",
    ),
    (
        "gradient",
        "\\nabla f = \\left( \\frac{\\partial f}{\\partial x}, \\frac{\\partial f}{\\partial y}, \\frac{\\partial f}{\\partial z} \\right)",
    ),
    (
        "partial_derivative",
        "\\frac{\\partial^2 u}{\\partial x \\partial y}",
    ),
    (
        "line_integral",
        "\\oint_C \\mathbf{F} \\cdot d\\mathbf{r} = \\iint_S (\\nabla \\times \\mathbf{F}) \\cdot d\\mathbf{S}",
    ),
    (
        "heat_equation",
        "\\frac{\\partial u}{\\partial t} = \\alpha \\nabla^2 u",
    ),
    (
        "wave_equation",
        "\\frac{\\partial^2 u}{\\partial t^2} = c^2 \\nabla^2 u",
    ),
    (
        "maxwell_gauss",
        "\\nabla \\cdot \\mathbf{E} = \\frac{\\rho}{\\varepsilon_0}",
    ),
    (
        "maxwell_faraday",
        "\\nabla \\times \\mathbf{E} = -\\frac{\\partial \\mathbf{B}}{\\partial t}",
    ),
    (
        "einstein_field",
        "G_{\\mu\\nu} + \\Lambda g_{\\mu\\nu} = \\frac{8\\pi G}{c^4} T_{\\mu\\nu}",
    ),
    (
        "schrodinger",
        "i\\hbar \\frac{\\partial}{\\partial t} \\Psi(x,t) = \\hat{H} \\Psi(x,t)",
    ),
    ("braket", "\\langle \\psi | \\hat{O} | \\phi \\rangle"),
    (
        "path_integral",
        "\\langle q_f | e^{-iHt/\\hbar} | q_i \\rangle = \\int_{q(0)=q_i}^{q(t)=q_f} e^{iS[q]/\\hbar} \\, \\mathcal{D}q",
    ),
    (
        "riemann_zeta",
        "\\zeta(s) = \\sum_{n=1}^{\\infty} \\frac{1}{n^s} = \\prod_{p \\text{ prime}} \\frac{1}{1 - p^{-s}}",
    ),
    (
        "euler_product",
        "\\zeta(s) = \\prod_{p} \\frac{1}{1 - p^{-s}}",
    ),
    ("eigenvalue", "A \\mathbf{v} = \\lambda \\mathbf{v}"),
    (
        "norm",
        "\\| \\mathbf{x} \\|_2 = \\sqrt{\\sum_{i=1}^{n} x_i^2}",
    ),
    (
        "inner_product",
        "\\langle u, v \\rangle = \\sum_{i=1}^{n} u_i \\overline{v_i}",
    ),
    (
        "gaussian",
        "f(x) = \\frac{1}{\\sigma\\sqrt{2\\pi}} \\exp\\left( -\\frac{(x-\\mu)^2}{2\\sigma^2} \\right)",
    ),
    (
        "expected_value",
        "\\mathbb{E}[X] = \\sum_{x} x \\cdot P(X = x)",
    ),
    (
        "variance",
        "\\operatorname{Var}(X) = \\mathbb{E}[(X - \\mu)^2]",
    ),
    (
        "set_union",
        "A \\cup B = \\{ x \\mid x \\in A \\text{ or } x \\in B \\}",
    ),
    (
        "forall_exists",
        "\\forall \\varepsilon > 0 \\quad \\exists \\delta > 0 : |x - a| < \\delta \\implies |f(x) - L| < \\varepsilon",
    ),
    ("accent_vector", "\\vec{v} = \\hat{i} + \\hat{j} + \\hat{k}"),
    ("accent_widehat", "\\widehat{AB}"),
    ("accent_widetilde", "\\widetilde{G}^{-1}(\\omega)"),
    (
        "aligned_equations",
        "\\begin{aligned} a &= b + c \\\\ d &= e + f \\end{aligned}",
    ),
    ("overset_def", "\\overset{\\text{def}}{=} x"),
    (
        "underbrace_sum",
        "\\underbrace{1 + 2 + \\cdots + n}_{n(n+1)/2}",
    ),
    (
        "math_font_variants",
        "\\mathsf{A} + \\mathtt{b} + \\mathcal{L} + \\bm{v}",
    ),
    (
        "arrow_relations",
        "\\hookrightarrow \\twoheadleftarrow \\rightleftharpoons \\leadsto",
    ),
    ("negated_relations", "\\nleq \\nsubseteq \\nprec \\nsim"),
];
