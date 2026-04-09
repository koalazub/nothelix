//! LaTeX-to-Unicode symbol table and overlay rendering for Helix.
//!
//! The symbol table is extracted verbatim from Julia's stdlib REPL package
//! (`stdlib/REPL/src/latex_symbols.jl`), which defines the same completions
//! available in the Julia REPL and Pluto.jl.  Typing `\alpha<Tab>` produces
//! `α`, `\in<Tab>` produces `∈`, and so on.
//!
//! Two FFI lookup functions are exposed:
//!   - `unicode-lookup`  — exact lookup by name (without the leading `\`)
//!   - `unicode-completions-for-prefix` — JSON array of all names that start
//!     with a given prefix, for use in a future completion popup
//!
//! The `latex-overlays` FFI function produces byte-offset overlay pairs for
//! Helix's overlay system, rendering LaTeX math commands as Unicode within
//! `$...$` regions.
//!
//! # Rendering pipeline
//!
//! Each math region goes through two passes:
//!
//! 1. **AST pass** — `mathlex::parse_latex_lenient` produces a semantic AST
//!    identifying fractions, subscripts, superscripts, matrices, etc.  This
//!    resolves ambiguities that the position-only scanner cannot (e.g., whether
//!    `_` introduces a subscript or is part of a matrix index).
//!
//! 2. **Overlay pass** — the manual byte-offset scanner walks the source text
//!    and emits `(offset, replacement)` pairs.  It consults the AST for
//!    semantic decisions but drives positioning from the source text directly,
//!    because mathlex does not yet provide per-node source spans.
//!
//! # Mathlex upstream contribution notes
//!
//! Our fork adds `SpannedExpression` and `parse_latex_spanned()`.  The
//! following features are needed upstream before overlays can walk the AST
//! alone:
//!
//! - **Per-node source spans** — only top-level spans available currently
//! - **`\begin{cases}`** — piecewise function environment not represented
//! - **`\text{...}`, `\mathrm{...}`, `\operatorname{...}`** — no text nodes
//! - **`\|` norm delimiter** — double vertical bars not parsed
//! - **`&`/`\\` separators** — no representation outside `Matrix`
//! - **Structured subscripts/superscripts** — `x_i` flattens to `Variable("x_i")`

use mathlex::Expression;
use serde_json::json;

// ─── Symbol table ─────────────────────────────────────────────────────────────
// Sorted by name so binary search works.  Extracted from Julia stdlib.

// 2544 entries
static SYMBOLS: &[(&str, &str)] = &[
    ("0/3", "↉"),
    ("1/", "⅟"),
    ("1/10", "⅒"),
    ("1/2", "½"),
    ("1/3", "⅓"),
    ("1/4", "¼"),
    ("1/5", "⅕"),
    ("1/6", "⅙"),
    ("1/7", "⅐"),
    ("1/8", "⅛"),
    ("1/9", "⅑"),
    ("2/3", "⅔"),
    ("2/5", "⅖"),
    ("3/4", "¾"),
    ("3/5", "⅗"),
    ("3/8", "⅜"),
    ("4/5", "⅘"),
    ("5/6", "⅚"),
    ("5/8", "⅝"),
    ("7/8", "⅞"),
    ("AA", "Å"),
    ("AE", "Æ"),
    ("Alpha", "Α"),
    ("And", "⩓"),
    ("Angle", "⦜"),
    ("Angstrom", "Å"),
    ("Beta", "Β"),
    ("Bot", "⫫"),
    ("Bumpeq", "≎"),
    ("Cap", "⋒"),
    ("Chi", "Χ"),
    ("Colon", "∷"),
    ("Coloneq", "⩴"),
    ("Cup", "⋓"),
    ("DDownarrow", "⟱"),
    ("DH", "Ð"),
    ("DJ", "Đ"),
    ("Dashv", "⫤"),
    ("Ddownarrow", "⤋"),
    ("Delta", "Δ"),
    ("Digamma", "Ϝ"),
    ("Doteq", "≑"),
    ("DownArrowBar", "⤓"),
    ("DownArrowUpArrow", "⇵"),
    ("DownLeftRightVector", "⥐"),
    ("DownLeftTeeVector", "⥞"),
    ("DownLeftVectorBar", "⥖"),
    ("DownRightTeeVector", "⥟"),
    ("DownRightVectorBar", "⥗"),
    ("Downarrow", "⇓"),
    ("ElOr", "⩖"),
    ("Elroang", "⦆"),
    ("Epsilon", "Ε"),
    ("Equal", "⩵"),
    ("Equiv", "≣"),
    ("Eta", "Η"),
    ("Finv", "Ⅎ"),
    ("Game", "⅁"),
    ("Gamma", "Γ"),
    ("H", "̋"),
    ("Im", "ℑ"),
    ("Iota", "Ι"),
    ("Kappa", "Κ"),
    ("Koppa", "Ϟ"),
    ("L", "Ł"),
    ("LLeftarrow", "⭅"),
    ("Lambda", "Λ"),
    ("Lap", "⧊"),
    ("Ldsh", "↲"),
    ("LeftDownTeeVector", "⥡"),
    ("LeftDownVectorBar", "⥙"),
    ("LeftRightVector", "⥎"),
    ("LeftTeeVector", "⥚"),
    ("LeftTriangleBar", "⧏"),
    ("LeftUpDownVector", "⥑"),
    ("LeftUpTeeVector", "⥠"),
    ("LeftUpVectorBar", "⥘"),
    ("LeftVectorBar", "⥒"),
    ("Leftarrow", "⇐"),
    ("Leftrightarrow", "⇔"),
    ("Lleftarrow", "⇚"),
    ("Longleftarrow", "⟸"),
    ("Longleftrightarrow", "⟺"),
    ("Longmapsfrom", "⟽"),
    ("Longmapsto", "⟾"),
    ("Longrightarrow", "⟹"),
    ("Lsh", "↰"),
    ("Mapsfrom", "⤆"),
    ("Mapsto", "⤇"),
    ("Mu", "Μ"),
    ("NG", "Ŋ"),
    ("Nearrow", "⇗"),
    ("NestedGreaterGreater", "⪢"),
    ("NestedLessLess", "⪡"),
    ("NotGreaterGreater", "≫̸"),
    ("NotLeftTriangleBar", "⧏̸"),
    ("NotLessLess", "≪̸"),
    ("NotNestedGreaterGreater", "⪢̸"),
    ("NotNestedLessLess", "⪡̸"),
    ("NotRightTriangleBar", "⧐̸"),
    ("NotSquareSubset", "⊏̸"),
    ("NotSquareSuperset", "⊐̸"),
    ("Nu", "Ν"),
    ("Nwarrow", "⇖"),
    ("O", "Ø"),
    ("OE", "Œ"),
    ("Omega", "Ω"),
    ("Omicron", "Ο"),
    ("Or", "⩔"),
    ("Otimes", "⨷"),
    ("P", "¶"),
    ("Phi", "Φ"),
    ("Pi", "Π"),
    ("Prec", "⪻"),
    ("PropertyLine", "⅊"),
    ("Psi", "Ψ"),
    ("QED", "∎"),
    ("RRightarrow", "⭆"),
    ("Rdsh", "↳"),
    ("Re", "ℜ"),
    ("ReverseUpEquilibrium", "⥯"),
    ("Rho", "Ρ"),
    ("RightDownTeeVector", "⥝"),
    ("RightDownVectorBar", "⥕"),
    ("RightTeeVector", "⥛"),
    ("RightTriangleBar", "⧐"),
    ("RightUpDownVector", "⥏"),
    ("RightUpTeeVector", "⥜"),
    ("RightUpVectorBar", "⥔"),
    ("RightVectorBar", "⥓"),
    ("Rightarrow", "⇒"),
    ("Rlarr", "⥂"),
    ("RoundImplies", "⥰"),
    ("Rrightarrow", "⇛"),
    ("Rsh", "↱"),
    ("RuleDelayed", "⧴"),
    ("S", "§"),
    ("Sampi", "Ϡ"),
    ("Searrow", "⇘"),
    ("Sigma", "Σ"),
    ("Sqcap", "⩎"),
    ("Sqcup", "⩏"),
    ("Stigma", "Ϛ"),
    ("Subset", "⋐"),
    ("Succ", "⪼"),
    ("Supset", "⋑"),
    ("Swarrow", "⇙"),
    ("TH", "Þ"),
    ("Tau", "Τ"),
    ("Theta", "Θ"),
    ("Times", "⨯"),
    ("Top", "⫪"),
    ("UUparrow", "⟰"),
    ("UpArrowBar", "⤒"),
    ("UpEquilibrium", "⥮"),
    ("Uparrow", "⇑"),
    ("Updownarrow", "⇕"),
    ("Upsilon", "Υ"),
    ("Uuparrow", "⤊"),
    ("VDash", "⊫"),
    ("Vdash", "⊩"),
    ("Vert", "‖"),
    ("Vvdash", "⊪"),
    ("Vvert", "⦀"),
    ("Xi", "Ξ"),
    ("Yup", "⅄"),
    ("Zbar", "Ƶ"),
    ("Zeta", "Ζ"),
    ("^!", "ꜝ"),
    ("^(", "⁽"),
    ("^)", "⁾"),
    ("^+", "⁺"),
    ("^-", "⁻"),
    ("^0", "⁰"),
    ("^1", "¹"),
    ("^2", "²"),
    ("^3", "³"),
    ("^4", "⁴"),
    ("^5", "⁵"),
    ("^6", "⁶"),
    ("^7", "⁷"),
    ("^8", "⁸"),
    ("^9", "⁹"),
    ("^=", "⁼"),
    ("^A", "ᴬ"),
    ("^B", "ᴮ"),
    ("^D", "ᴰ"),
    ("^E", "ᴱ"),
    ("^G", "ᴳ"),
    ("^H", "ᴴ"),
    ("^I", "ᴵ"),
    ("^J", "ᴶ"),
    ("^K", "ᴷ"),
    ("^L", "ᴸ"),
    ("^M", "ᴹ"),
    ("^N", "ᴺ"),
    ("^O", "ᴼ"),
    ("^P", "ᴾ"),
    ("^R", "ᴿ"),
    ("^T", "ᵀ"),
    ("^U", "ᵁ"),
    ("^V", "ⱽ"),
    ("^W", "ᵂ"),
    ("^a", "ᵃ"),
    ("^alpha", "ᵅ"),
    ("^b", "ᵇ"),
    ("^beta", "ᵝ"),
    ("^c", "ᶜ"),
    ("^chi", "ᵡ"),
    ("^d", "ᵈ"),
    ("^delta", "ᵟ"),
    ("^downarrow", "ꜜ"),
    ("^e", "ᵉ"),
    ("^epsilon", "ᵋ"),
    ("^f", "ᶠ"),
    ("^g", "ᵍ"),
    ("^gamma", "ᵞ"),
    ("^h", "ʰ"),
    ("^i", "ⁱ"),
    ("^iota", "ᶥ"),
    ("^j", "ʲ"),
    ("^k", "ᵏ"),
    ("^l", "ˡ"),
    ("^ltphi", "ᶲ"),
    ("^m", "ᵐ"),
    ("^n", "ⁿ"),
    ("^o", "ᵒ"),
    ("^p", "ᵖ"),
    ("^phi", "ᵠ"),
    ("^r", "ʳ"),
    ("^s", "ˢ"),
    ("^t", "ᵗ"),
    ("^theta", "ᶿ"),
    ("^u", "ᵘ"),
    ("^uparrow", "ꜛ"),
    ("^v", "ᵛ"),
    ("^w", "ʷ"),
    ("^x", "ˣ"),
    ("^y", "ʸ"),
    ("^z", "ᶻ"),
    ("_(", "₍"),
    ("_)", "₎"),
    ("_+", "₊"),
    ("_-", "₋"),
    ("_0", "₀"),
    ("_1", "₁"),
    ("_2", "₂"),
    ("_3", "₃"),
    ("_4", "₄"),
    ("_5", "₅"),
    ("_6", "₆"),
    ("_7", "₇"),
    ("_8", "₈"),
    ("_9", "₉"),
    ("_<", "˱"),
    ("_=", "₌"),
    ("_>", "˲"),
    ("_a", "ₐ"),
    ("_beta", "ᵦ"),
    ("_chi", "ᵪ"),
    ("_e", "ₑ"),
    ("_gamma", "ᵧ"),
    ("_h", "ₕ"),
    ("_i", "ᵢ"),
    ("_j", "ⱼ"),
    ("_k", "ₖ"),
    ("_l", "ₗ"),
    ("_m", "ₘ"),
    ("_n", "ₙ"),
    ("_o", "ₒ"),
    ("_p", "ₚ"),
    ("_phi", "ᵩ"),
    ("_r", "ᵣ"),
    ("_rho", "ᵨ"),
    ("_s", "ₛ"),
    ("_schwa", "ₔ"),
    ("_t", "ₜ"),
    ("_u", "ᵤ"),
    ("_v", "ᵥ"),
    ("_x", "ₓ"),
    ("aa", "å"),
    ("accurrent", "⏦"),
    ("acidfree", "♾"),
    ("acute", "́"),
    ("adots", "⋰"),
    ("ae", "æ"),
    ("aleph", "ℵ"),
    ("allequal", "≌"),
    ("alpha", "α"),
    ("amalg", "⨿"),
    ("angdnr", "⦟"),
    ("angle", "∠"),
    ("angles", "⦞"),
    ("angleubar", "⦤"),
    ("annuity", "⃧"),
    ("approx", "≈"),
    ("approxeq", "≊"),
    ("approxeqq", "⩰"),
    ("approxnotequal", "≆"),
    ("aquarius", "♒"),
    ("arceq", "≘"),
    ("aries", "♈"),
    ("ast", "∗"),
    ("asteq", "⩮"),
    ("asteraccent", "⃰"),
    ("astrosun", "☉"),
    ("asymp", "≍"),
    ("awint", "⨑"),
    ("backepsilon", "϶"),
    ("backppprime", "‷"),
    ("backpprime", "‶"),
    ("backprime", "‵"),
    ("backsim", "∽"),
    ("backsimeq", "⋍"),
    ("bagmember", "⋿"),
    ("bar", "̄"),
    ("barcap", "⩃"),
    ("barcup", "⩂"),
    ("barleftarrow", "⇤"),
    ("barleftarrowrightarrowbar", "↹"),
    ("barovernorthwestarrow", "↸"),
    ("barrightarrowdiamond", "⤠"),
    ("barvee", "⊽"),
    ("barwedge", "⊼"),
    ("bbA", "𝔸"),
    ("bbB", "𝔹"),
    ("bbC", "ℂ"),
    ("bbD", "𝔻"),
    ("bbE", "𝔼"),
    ("bbF", "𝔽"),
    ("bbG", "𝔾"),
    ("bbGamma", "ℾ"),
    ("bbH", "ℍ"),
    ("bbI", "𝕀"),
    ("bbJ", "𝕁"),
    ("bbK", "𝕂"),
    ("bbL", "𝕃"),
    ("bbM", "𝕄"),
    ("bbN", "ℕ"),
    ("bbO", "𝕆"),
    ("bbP", "ℙ"),
    ("bbPi", "ℿ"),
    ("bbQ", "ℚ"),
    ("bbR", "ℝ"),
    ("bbS", "𝕊"),
    ("bbT", "𝕋"),
    ("bbU", "𝕌"),
    ("bbV", "𝕍"),
    ("bbW", "𝕎"),
    ("bbX", "𝕏"),
    ("bbY", "𝕐"),
    ("bbZ", "ℤ"),
    ("bba", "𝕒"),
    ("bbb", "𝕓"),
    ("bbc", "𝕔"),
    ("bbd", "𝕕"),
    ("bbe", "𝕖"),
    ("bbeight", "𝟠"),
    ("bbf", "𝕗"),
    ("bbfive", "𝟝"),
    ("bbfour", "𝟜"),
    ("bbg", "𝕘"),
    ("bbgamma", "ℽ"),
    ("bbh", "𝕙"),
    ("bbi", "𝕚"),
    ("bbiD", "ⅅ"),
    ("bbid", "ⅆ"),
    ("bbie", "ⅇ"),
    ("bbii", "ⅈ"),
    ("bbij", "ⅉ"),
    ("bbj", "𝕛"),
    ("bbk", "𝕜"),
    ("bbl", "𝕝"),
    ("bbm", "𝕞"),
    ("bbn", "𝕟"),
    ("bbnine", "𝟡"),
    ("bbo", "𝕠"),
    ("bbone", "𝟙"),
    ("bbp", "𝕡"),
    ("bbpi", "ℼ"),
    ("bbq", "𝕢"),
    ("bbr", "𝕣"),
    ("bbrktbrk", "⎶"),
    ("bbs", "𝕤"),
    ("bbsemi", "⨟"),
    ("bbseven", "𝟟"),
    ("bbsix", "𝟞"),
    ("bbsum", "⅀"),
    ("bbt", "𝕥"),
    ("bbthree", "𝟛"),
    ("bbtwo", "𝟚"),
    ("bbu", "𝕦"),
    ("bbv", "𝕧"),
    ("bbw", "𝕨"),
    ("bbx", "𝕩"),
    ("bby", "𝕪"),
    ("bbz", "𝕫"),
    ("bbzero", "𝟘"),
    ("because", "∵"),
    ("benzenr", "⏣"),
    ("beta", "β"),
    ("beth", "ℶ"),
    ("between", "≬"),
    ("bfA", "𝐀"),
    ("bfAlpha", "𝚨"),
    ("bfB", "𝐁"),
    ("bfBeta", "𝚩"),
    ("bfC", "𝐂"),
    ("bfChi", "𝚾"),
    ("bfD", "𝐃"),
    ("bfDelta", "𝚫"),
    ("bfE", "𝐄"),
    ("bfEpsilon", "𝚬"),
    ("bfEta", "𝚮"),
    ("bfF", "𝐅"),
    ("bfG", "𝐆"),
    ("bfGamma", "𝚪"),
    ("bfH", "𝐇"),
    ("bfI", "𝐈"),
    ("bfIota", "𝚰"),
    ("bfJ", "𝐉"),
    ("bfK", "𝐊"),
    ("bfKappa", "𝚱"),
    ("bfL", "𝐋"),
    ("bfLambda", "𝚲"),
    ("bfM", "𝐌"),
    ("bfMu", "𝚳"),
    ("bfN", "𝐍"),
    ("bfNu", "𝚴"),
    ("bfO", "𝐎"),
    ("bfOmega", "𝛀"),
    ("bfOmicron", "𝚶"),
    ("bfP", "𝐏"),
    ("bfPhi", "𝚽"),
    ("bfPi", "𝚷"),
    ("bfPsi", "𝚿"),
    ("bfQ", "𝐐"),
    ("bfR", "𝐑"),
    ("bfRho", "𝚸"),
    ("bfS", "𝐒"),
    ("bfSigma", "𝚺"),
    ("bfT", "𝐓"),
    ("bfTau", "𝚻"),
    ("bfTheta", "𝚯"),
    ("bfU", "𝐔"),
    ("bfUpsilon", "𝚼"),
    ("bfV", "𝐕"),
    ("bfW", "𝐖"),
    ("bfX", "𝐗"),
    ("bfXi", "𝚵"),
    ("bfY", "𝐘"),
    ("bfZ", "𝐙"),
    ("bfZeta", "𝚭"),
    ("bfa", "𝐚"),
    ("bfalpha", "𝛂"),
    ("bfb", "𝐛"),
    ("bfbeta", "𝛃"),
    ("bfc", "𝐜"),
    ("bfchi", "𝛘"),
    ("bfd", "𝐝"),
    ("bfdelta", "𝛅"),
    ("bfe", "𝐞"),
    ("bfeight", "𝟖"),
    ("bfepsilon", "𝛜"),
    ("bfeta", "𝛈"),
    ("bff", "𝐟"),
    ("bffive", "𝟓"),
    ("bffour", "𝟒"),
    ("bfg", "𝐠"),
    ("bfgamma", "𝛄"),
    ("bfh", "𝐡"),
    ("bfi", "𝐢"),
    ("bfiota", "𝛊"),
    ("bfj", "𝐣"),
    ("bfk", "𝐤"),
    ("bfkappa", "𝛋"),
    ("bfl", "𝐥"),
    ("bflambda", "𝛌"),
    ("bfm", "𝐦"),
    ("bfmu", "𝛍"),
    ("bfn", "𝐧"),
    ("bfnabla", "𝛁"),
    ("bfnine", "𝟗"),
    ("bfnu", "𝛎"),
    ("bfo", "𝐨"),
    ("bfomega", "𝛚"),
    ("bfomicron", "𝛐"),
    ("bfone", "𝟏"),
    ("bfp", "𝐩"),
    ("bfpartial", "𝛛"),
    ("bfphi", "𝛟"),
    ("bfpi", "𝛑"),
    ("bfpsi", "𝛙"),
    ("bfq", "𝐪"),
    ("bfr", "𝐫"),
    ("bfrakA", "𝕬"),
    ("bfrakB", "𝕭"),
    ("bfrakC", "𝕮"),
    ("bfrakD", "𝕯"),
    ("bfrakE", "𝕰"),
    ("bfrakF", "𝕱"),
    ("bfrakG", "𝕲"),
    ("bfrakH", "𝕳"),
    ("bfrakI", "𝕴"),
    ("bfrakJ", "𝕵"),
    ("bfrakK", "𝕶"),
    ("bfrakL", "𝕷"),
    ("bfrakM", "𝕸"),
    ("bfrakN", "𝕹"),
    ("bfrakO", "𝕺"),
    ("bfrakP", "𝕻"),
    ("bfrakQ", "𝕼"),
    ("bfrakR", "𝕽"),
    ("bfrakS", "𝕾"),
    ("bfrakT", "𝕿"),
    ("bfrakU", "𝖀"),
    ("bfrakV", "𝖁"),
    ("bfrakW", "𝖂"),
    ("bfrakX", "𝖃"),
    ("bfrakY", "𝖄"),
    ("bfrakZ", "𝖅"),
    ("bfraka", "𝖆"),
    ("bfrakb", "𝖇"),
    ("bfrakc", "𝖈"),
    ("bfrakd", "𝖉"),
    ("bfrake", "𝖊"),
    ("bfrakf", "𝖋"),
    ("bfrakg", "𝖌"),
    ("bfrakh", "𝖍"),
    ("bfraki", "𝖎"),
    ("bfrakj", "𝖏"),
    ("bfrakk", "𝖐"),
    ("bfrakl", "𝖑"),
    ("bfrakm", "𝖒"),
    ("bfrakn", "𝖓"),
    ("bfrako", "𝖔"),
    ("bfrakp", "𝖕"),
    ("bfrakq", "𝖖"),
    ("bfrakr", "𝖗"),
    ("bfraks", "𝖘"),
    ("bfrakt", "𝖙"),
    ("bfraku", "𝖚"),
    ("bfrakv", "𝖛"),
    ("bfrakw", "𝖜"),
    ("bfrakx", "𝖝"),
    ("bfraky", "𝖞"),
    ("bfrakz", "𝖟"),
    ("bfrho", "𝛒"),
    ("bfs", "𝐬"),
    ("bfseven", "𝟕"),
    ("bfsigma", "𝛔"),
    ("bfsix", "𝟔"),
    ("bft", "𝐭"),
    ("bftau", "𝛕"),
    ("bftheta", "𝛉"),
    ("bfthree", "𝟑"),
    ("bftwo", "𝟐"),
    ("bfu", "𝐮"),
    ("bfupsilon", "𝛖"),
    ("bfv", "𝐯"),
    ("bfvarTheta", "𝚹"),
    ("bfvarepsilon", "𝛆"),
    ("bfvarkappa", "𝛞"),
    ("bfvarphi", "𝛗"),
    ("bfvarpi", "𝛡"),
    ("bfvarrho", "𝛠"),
    ("bfvarsigma", "𝛓"),
    ("bfvartheta", "𝛝"),
    ("bfw", "𝐰"),
    ("bfx", "𝐱"),
    ("bfxi", "𝛏"),
    ("bfy", "𝐲"),
    ("bfz", "𝐳"),
    ("bfzero", "𝟎"),
    ("bfzeta", "𝛇"),
    ("biA", "𝑨"),
    ("biAlpha", "𝜜"),
    ("biB", "𝑩"),
    ("biBeta", "𝜝"),
    ("biC", "𝑪"),
    ("biChi", "𝜲"),
    ("biD", "𝑫"),
    ("biDelta", "𝜟"),
    ("biE", "𝑬"),
    ("biEpsilon", "𝜠"),
    ("biEta", "𝜢"),
    ("biF", "𝑭"),
    ("biG", "𝑮"),
    ("biGamma", "𝜞"),
    ("biH", "𝑯"),
    ("biI", "𝑰"),
    ("biIota", "𝜤"),
    ("biJ", "𝑱"),
    ("biK", "𝑲"),
    ("biKappa", "𝜥"),
    ("biL", "𝑳"),
    ("biLambda", "𝜦"),
    ("biM", "𝑴"),
    ("biMu", "𝜧"),
    ("biN", "𝑵"),
    ("biNu", "𝜨"),
    ("biO", "𝑶"),
    ("biOmega", "𝜴"),
    ("biOmicron", "𝜪"),
    ("biP", "𝑷"),
    ("biPhi", "𝜱"),
    ("biPi", "𝜫"),
    ("biPsi", "𝜳"),
    ("biQ", "𝑸"),
    ("biR", "𝑹"),
    ("biRho", "𝜬"),
    ("biS", "𝑺"),
    ("biSigma", "𝜮"),
    ("biT", "𝑻"),
    ("biTau", "𝜯"),
    ("biTheta", "𝜣"),
    ("biU", "𝑼"),
    ("biUpsilon", "𝜰"),
    ("biV", "𝑽"),
    ("biW", "𝑾"),
    ("biX", "𝑿"),
    ("biXi", "𝜩"),
    ("biY", "𝒀"),
    ("biZ", "𝒁"),
    ("biZeta", "𝜡"),
    ("bia", "𝒂"),
    ("bialpha", "𝜶"),
    ("bib", "𝒃"),
    ("bibeta", "𝜷"),
    ("bic", "𝒄"),
    ("bichi", "𝝌"),
    ("bid", "𝒅"),
    ("bidelta", "𝜹"),
    ("bie", "𝒆"),
    ("biepsilon", "𝝐"),
    ("bieta", "𝜼"),
    ("bif", "𝒇"),
    ("big", "𝒈"),
    ("bigamma", "𝜸"),
    ("bigblacktriangledown", "▼"),
    ("bigblacktriangleup", "▲"),
    ("bigbot", "⟘"),
    ("bigcap", "⋂"),
    ("bigcirc", "○"),
    ("bigcup", "⋃"),
    ("bigcupdot", "⨃"),
    ("bigodot", "⨀"),
    ("bigoplus", "⨁"),
    ("bigotimes", "⨂"),
    ("bigslopedvee", "⩗"),
    ("bigslopedwedge", "⩘"),
    ("bigsqcap", "⨅"),
    ("bigsqcup", "⨆"),
    ("bigstar", "★"),
    ("bigtimes", "⨉"),
    ("bigtop", "⟙"),
    ("bigtriangledown", "▽"),
    ("bigtriangleup", "△"),
    ("biguplus", "⨄"),
    ("bigvee", "⋁"),
    ("bigwedge", "⋀"),
    ("bigwhitestar", "☆"),
    ("bih", "𝒉"),
    ("bii", "𝒊"),
    ("biiota", "𝜾"),
    ("bij", "𝒋"),
    ("bik", "𝒌"),
    ("bikappa", "𝜿"),
    ("bil", "𝒍"),
    ("bilambda", "𝝀"),
    ("bim", "𝒎"),
    ("bimu", "𝝁"),
    ("bin", "𝒏"),
    ("binabla", "𝜵"),
    ("binu", "𝝂"),
    ("bio", "𝒐"),
    ("biomega", "𝝎"),
    ("biomicron", "𝝄"),
    ("bip", "𝒑"),
    ("bipartial", "𝝏"),
    ("biphi", "𝝓"),
    ("bipi", "𝝅"),
    ("bipsi", "𝝍"),
    ("biq", "𝒒"),
    ("bir", "𝒓"),
    ("birho", "𝝆"),
    ("bis", "𝒔"),
    ("bisansA", "𝘼"),
    ("bisansAlpha", "𝞐"),
    ("bisansB", "𝘽"),
    ("bisansBeta", "𝞑"),
    ("bisansC", "𝘾"),
    ("bisansChi", "𝞦"),
    ("bisansD", "𝘿"),
    ("bisansDelta", "𝞓"),
    ("bisansE", "𝙀"),
    ("bisansEpsilon", "𝞔"),
    ("bisansEta", "𝞖"),
    ("bisansF", "𝙁"),
    ("bisansG", "𝙂"),
    ("bisansGamma", "𝞒"),
    ("bisansH", "𝙃"),
    ("bisansI", "𝙄"),
    ("bisansIota", "𝞘"),
    ("bisansJ", "𝙅"),
    ("bisansK", "𝙆"),
    ("bisansKappa", "𝞙"),
    ("bisansL", "𝙇"),
    ("bisansLambda", "𝞚"),
    ("bisansM", "𝙈"),
    ("bisansMu", "𝞛"),
    ("bisansN", "𝙉"),
    ("bisansNu", "𝞜"),
    ("bisansO", "𝙊"),
    ("bisansOmega", "𝞨"),
    ("bisansOmicron", "𝞞"),
    ("bisansP", "𝙋"),
    ("bisansPhi", "𝞥"),
    ("bisansPi", "𝞟"),
    ("bisansPsi", "𝞧"),
    ("bisansQ", "𝙌"),
    ("bisansR", "𝙍"),
    ("bisansRho", "𝞠"),
    ("bisansS", "𝙎"),
    ("bisansSigma", "𝞢"),
    ("bisansT", "𝙏"),
    ("bisansTau", "𝞣"),
    ("bisansTheta", "𝞗"),
    ("bisansU", "𝙐"),
    ("bisansUpsilon", "𝞤"),
    ("bisansV", "𝙑"),
    ("bisansW", "𝙒"),
    ("bisansX", "𝙓"),
    ("bisansXi", "𝞝"),
    ("bisansY", "𝙔"),
    ("bisansZ", "𝙕"),
    ("bisansZeta", "𝞕"),
    ("bisansa", "𝙖"),
    ("bisansalpha", "𝞪"),
    ("bisansb", "𝙗"),
    ("bisansbeta", "𝞫"),
    ("bisansc", "𝙘"),
    ("bisanschi", "𝟀"),
    ("bisansd", "𝙙"),
    ("bisansdelta", "𝞭"),
    ("bisanse", "𝙚"),
    ("bisansepsilon", "𝟄"),
    ("bisanseta", "𝞰"),
    ("bisansf", "𝙛"),
    ("bisansg", "𝙜"),
    ("bisansgamma", "𝞬"),
    ("bisansh", "𝙝"),
    ("bisansi", "𝙞"),
    ("bisansiota", "𝞲"),
    ("bisansj", "𝙟"),
    ("bisansk", "𝙠"),
    ("bisanskappa", "𝞳"),
    ("bisansl", "𝙡"),
    ("bisanslambda", "𝞴"),
    ("bisansm", "𝙢"),
    ("bisansmu", "𝞵"),
    ("bisansn", "𝙣"),
    ("bisansnabla", "𝞩"),
    ("bisansnu", "𝞶"),
    ("bisanso", "𝙤"),
    ("bisansomega", "𝟂"),
    ("bisansomicron", "𝞸"),
    ("bisansp", "𝙥"),
    ("bisanspartial", "𝟃"),
    ("bisansphi", "𝟇"),
    ("bisanspi", "𝞹"),
    ("bisanspsi", "𝟁"),
    ("bisansq", "𝙦"),
    ("bisansr", "𝙧"),
    ("bisansrho", "𝞺"),
    ("bisanss", "𝙨"),
    ("bisanssigma", "𝞼"),
    ("bisanst", "𝙩"),
    ("bisanstau", "𝞽"),
    ("bisanstheta", "𝞱"),
    ("bisansu", "𝙪"),
    ("bisansupsilon", "𝞾"),
    ("bisansv", "𝙫"),
    ("bisansvarTheta", "𝞡"),
    ("bisansvarepsilon", "𝞮"),
    ("bisansvarkappa", "𝟆"),
    ("bisansvarphi", "𝞿"),
    ("bisansvarpi", "𝟉"),
    ("bisansvarrho", "𝟈"),
    ("bisansvarsigma", "𝞻"),
    ("bisansvartheta", "𝟅"),
    ("bisansw", "𝙬"),
    ("bisansx", "𝙭"),
    ("bisansxi", "𝞷"),
    ("bisansy", "𝙮"),
    ("bisansz", "𝙯"),
    ("bisanszeta", "𝞯"),
    ("bisigma", "𝝈"),
    ("bit", "𝒕"),
    ("bitau", "𝝉"),
    ("bitheta", "𝜽"),
    ("biu", "𝒖"),
    ("biupsilon", "𝝊"),
    ("biv", "𝒗"),
    ("bivarTheta", "𝜭"),
    ("bivarepsilon", "𝜺"),
    ("bivarkappa", "𝝒"),
    ("bivarphi", "𝝋"),
    ("bivarpi", "𝝕"),
    ("bivarrho", "𝝔"),
    ("bivarsigma", "𝝇"),
    ("bivartheta", "𝝑"),
    ("biw", "𝒘"),
    ("bix", "𝒙"),
    ("bixi", "𝝃"),
    ("biy", "𝒚"),
    ("biz", "𝒛"),
    ("bizeta", "𝜻"),
    ("bkarow", "⤍"),
    ("blackcircledrightdot", "⚈"),
    ("blackcircledtwodots", "⚉"),
    ("blackcircleulquadwhite", "◕"),
    ("blackinwhitediamond", "◈"),
    ("blackinwhitesquare", "▣"),
    ("blacklefthalfcircle", "◖"),
    ("blacklozenge", "⧫"),
    ("blackpointerleft", "◄"),
    ("blackpointerright", "►"),
    ("blackrighthalfcircle", "◗"),
    ("blacksmiley", "☻"),
    ("blacksquare", "■"),
    ("blacktriangle", "▴"),
    ("blacktriangledown", "▾"),
    ("blacktriangleleft", "◀"),
    ("blacktriangleright", "▶"),
    ("blanksymbol", "␢"),
    ("blkhorzoval", "⬬"),
    ("blkvertoval", "⬮"),
    ("blockfull", "█"),
    ("blockhalfshaded", "▒"),
    ("blocklefthalf", "▌"),
    ("blocklowhalf", "▄"),
    ("blockqtrshaded", "░"),
    ("blockrighthalf", "▐"),
    ("blockthreeqtrshaded", "▓"),
    ("blockuphalf", "▀"),
    ("bot", "⊥"),
    ("botsemicircle", "◡"),
    ("bowtie", "⋈"),
    ("boxast", "⧆"),
    ("boxbar", "◫"),
    ("boxbslash", "⧅"),
    ("boxcircle", "⧇"),
    ("boxdiag", "⧄"),
    ("boxdot", "⊡"),
    ("boxminus", "⊟"),
    ("boxplus", "⊞"),
    ("boxquestion", "⍰"),
    ("boxtimes", "⊠"),
    ("boxupcaret", "⍓"),
    ("breve", "̆"),
    ("brokenbar", "¦"),
    ("bsansA", "𝗔"),
    ("bsansAlpha", "𝝖"),
    ("bsansB", "𝗕"),
    ("bsansBeta", "𝝗"),
    ("bsansC", "𝗖"),
    ("bsansChi", "𝝬"),
    ("bsansD", "𝗗"),
    ("bsansDelta", "𝝙"),
    ("bsansE", "𝗘"),
    ("bsansEpsilon", "𝝚"),
    ("bsansEta", "𝝜"),
    ("bsansF", "𝗙"),
    ("bsansG", "𝗚"),
    ("bsansGamma", "𝝘"),
    ("bsansH", "𝗛"),
    ("bsansI", "𝗜"),
    ("bsansIota", "𝝞"),
    ("bsansJ", "𝗝"),
    ("bsansK", "𝗞"),
    ("bsansKappa", "𝝟"),
    ("bsansL", "𝗟"),
    ("bsansLambda", "𝝠"),
    ("bsansM", "𝗠"),
    ("bsansMu", "𝝡"),
    ("bsansN", "𝗡"),
    ("bsansNu", "𝝢"),
    ("bsansO", "𝗢"),
    ("bsansOmega", "𝝮"),
    ("bsansOmicron", "𝝤"),
    ("bsansP", "𝗣"),
    ("bsansPhi", "𝝫"),
    ("bsansPi", "𝝥"),
    ("bsansPsi", "𝝭"),
    ("bsansQ", "𝗤"),
    ("bsansR", "𝗥"),
    ("bsansRho", "𝝦"),
    ("bsansS", "𝗦"),
    ("bsansSigma", "𝝨"),
    ("bsansT", "𝗧"),
    ("bsansTau", "𝝩"),
    ("bsansTheta", "𝝝"),
    ("bsansU", "𝗨"),
    ("bsansUpsilon", "𝝪"),
    ("bsansV", "𝗩"),
    ("bsansW", "𝗪"),
    ("bsansX", "𝗫"),
    ("bsansXi", "𝝣"),
    ("bsansY", "𝗬"),
    ("bsansZ", "𝗭"),
    ("bsansZeta", "𝝛"),
    ("bsansa", "𝗮"),
    ("bsansalpha", "𝝰"),
    ("bsansb", "𝗯"),
    ("bsansbeta", "𝝱"),
    ("bsansc", "𝗰"),
    ("bsanschi", "𝞆"),
    ("bsansd", "𝗱"),
    ("bsansdelta", "𝝳"),
    ("bsanse", "𝗲"),
    ("bsanseight", "𝟴"),
    ("bsansepsilon", "𝞊"),
    ("bsanseta", "𝝶"),
    ("bsansf", "𝗳"),
    ("bsansfive", "𝟱"),
    ("bsansfour", "𝟰"),
    ("bsansg", "𝗴"),
    ("bsansgamma", "𝝲"),
    ("bsansh", "𝗵"),
    ("bsansi", "𝗶"),
    ("bsansiota", "𝝸"),
    ("bsansj", "𝗷"),
    ("bsansk", "𝗸"),
    ("bsanskappa", "𝝹"),
    ("bsansl", "𝗹"),
    ("bsanslambda", "𝝺"),
    ("bsansm", "𝗺"),
    ("bsansmu", "𝝻"),
    ("bsansn", "𝗻"),
    ("bsansnabla", "𝝯"),
    ("bsansnine", "𝟵"),
    ("bsansnu", "𝝼"),
    ("bsanso", "𝗼"),
    ("bsansomega", "𝞈"),
    ("bsansomicron", "𝝾"),
    ("bsansone", "𝟭"),
    ("bsansp", "𝗽"),
    ("bsanspartial", "𝞉"),
    ("bsansphi", "𝞍"),
    ("bsanspi", "𝝿"),
    ("bsanspsi", "𝞇"),
    ("bsansq", "𝗾"),
    ("bsansr", "𝗿"),
    ("bsansrho", "𝞀"),
    ("bsanss", "𝘀"),
    ("bsansseven", "𝟳"),
    ("bsanssigma", "𝞂"),
    ("bsanssix", "𝟲"),
    ("bsanst", "𝘁"),
    ("bsanstau", "𝞃"),
    ("bsanstheta", "𝝷"),
    ("bsansthree", "𝟯"),
    ("bsanstwo", "𝟮"),
    ("bsansu", "𝘂"),
    ("bsansupsilon", "𝞄"),
    ("bsansv", "𝘃"),
    ("bsansvarTheta", "𝝧"),
    ("bsansvarepsilon", "𝝴"),
    ("bsansvarkappa", "𝞌"),
    ("bsansvarphi", "𝞅"),
    ("bsansvarpi", "𝞏"),
    ("bsansvarrho", "𝞎"),
    ("bsansvarsigma", "𝞁"),
    ("bsansvartheta", "𝞋"),
    ("bsansw", "𝘄"),
    ("bsansx", "𝘅"),
    ("bsansxi", "𝝽"),
    ("bsansy", "𝘆"),
    ("bsansz", "𝘇"),
    ("bsanszero", "𝟬"),
    ("bsanszeta", "𝝵"),
    ("bscrA", "𝓐"),
    ("bscrB", "𝓑"),
    ("bscrC", "𝓒"),
    ("bscrD", "𝓓"),
    ("bscrE", "𝓔"),
    ("bscrF", "𝓕"),
    ("bscrG", "𝓖"),
    ("bscrH", "𝓗"),
    ("bscrI", "𝓘"),
    ("bscrJ", "𝓙"),
    ("bscrK", "𝓚"),
    ("bscrL", "𝓛"),
    ("bscrM", "𝓜"),
    ("bscrN", "𝓝"),
    ("bscrO", "𝓞"),
    ("bscrP", "𝓟"),
    ("bscrQ", "𝓠"),
    ("bscrR", "𝓡"),
    ("bscrS", "𝓢"),
    ("bscrT", "𝓣"),
    ("bscrU", "𝓤"),
    ("bscrV", "𝓥"),
    ("bscrW", "𝓦"),
    ("bscrX", "𝓧"),
    ("bscrY", "𝓨"),
    ("bscrZ", "𝓩"),
    ("bscra", "𝓪"),
    ("bscrb", "𝓫"),
    ("bscrc", "𝓬"),
    ("bscrd", "𝓭"),
    ("bscre", "𝓮"),
    ("bscrf", "𝓯"),
    ("bscrg", "𝓰"),
    ("bscrh", "𝓱"),
    ("bscri", "𝓲"),
    ("bscrj", "𝓳"),
    ("bscrk", "𝓴"),
    ("bscrl", "𝓵"),
    ("bscrm", "𝓶"),
    ("bscrn", "𝓷"),
    ("bscro", "𝓸"),
    ("bscrp", "𝓹"),
    ("bscrq", "𝓺"),
    ("bscrr", "𝓻"),
    ("bscrs", "𝓼"),
    ("bscrt", "𝓽"),
    ("bscru", "𝓾"),
    ("bscrv", "𝓿"),
    ("bscrw", "𝔀"),
    ("bscrx", "𝔁"),
    ("bscry", "𝔂"),
    ("bscrz", "𝔃"),
    ("bsimilarleftarrow", "⭁"),
    ("bsimilarrightarrow", "⭇"),
    ("bsolhsub", "⟈"),
    ("btdl", "ɬ"),
    ("btimes", "⨲"),
    ("bullet", "•"),
    ("bullseye", "◎"),
    ("bumpeq", "≏"),
    ("bumpeqq", "⪮"),
    ("c", "̧"),
    ("cancer", "♋"),
    ("candra", "̐"),
    ("cap", "∩"),
    ("capdot", "⩀"),
    ("capricornus", "♑"),
    ("capwedge", "⩄"),
    ("carriagereturn", "↵"),
    ("cbrt", "∛"),
    ("cdot", "⋅"),
    ("cdotp", "·"),
    ("cdots", "⋯"),
    ("check", "̌"),
    ("checkmark", "✓"),
    ("chi", "χ"),
    ("circ", "∘"),
    ("circeq", "≗"),
    ("circlearrowleft", "↺"),
    ("circlearrowright", "↻"),
    ("circledR", "®"),
    ("circledS", "Ⓢ"),
    ("circledast", "⊛"),
    ("circledbullet", "⦿"),
    ("circledcirc", "⊚"),
    ("circleddash", "⊝"),
    ("circledequal", "⊜"),
    ("circledparallel", "⦷"),
    ("circledrightdot", "⚆"),
    ("circledstar", "✪"),
    ("circledtwodots", "⚇"),
    ("circledwhitebullet", "⦾"),
    ("circlellquad", "◵"),
    ("circlelrquad", "◶"),
    ("circleonleftarrow", "⬰"),
    ("circleonrightarrow", "⇴"),
    ("circletophalfblack", "◓"),
    ("circleulquad", "◴"),
    ("circleurquad", "◷"),
    ("circleurquadblack", "◔"),
    ("circlevertfill", "◍"),
    ("cirfb", "◒"),
    ("cirfl", "◐"),
    ("cirfnint", "⨐"),
    ("cirfr", "◑"),
    ("clefc", "𝄡"),
    ("cleff", "𝄢"),
    ("cleff8va", "𝄣"),
    ("cleff8vb", "𝄤"),
    ("clefg", "𝄞"),
    ("clefg8va", "𝄟"),
    ("clefg8vb", "𝄠"),
    ("clockoint", "⨏"),
    ("clomeg", "ɷ"),
    ("closedvarcap", "⩍"),
    ("closedvarcup", "⩌"),
    ("closedvarcupsmashprod", "⩐"),
    ("clubsuit", "♣"),
    ("clwintegral", "∱"),
    ("coda", "𝄌"),
    ("coloneq", "≔"),
    ("commaminus", "⨩"),
    ("complement", "∁"),
    ("cong", "≅"),
    ("congdot", "⩭"),
    ("conictaper", "⌲"),
    ("conjquant", "⨇"),
    ("coprod", "∐"),
    ("copyright", "©"),
    ("csub", "⫏"),
    ("csube", "⫑"),
    ("csup", "⫐"),
    ("csupe", "⫒"),
    ("cup", "∪"),
    ("cupdot", "⊍"),
    ("cupvee", "⩅"),
    ("curlyeqprec", "⋞"),
    ("curlyeqsucc", "⋟"),
    ("curlyvee", "⋎"),
    ("curlywedge", "⋏"),
    ("curvearrowleft", "↶"),
    ("curvearrowright", "↷"),
    ("dacapo", "𝄊"),
    ("dagger", "†"),
    ("daleth", "ℸ"),
    ("dalsegno", "𝄉"),
    ("danger", "☡"),
    ("dashV", "⫣"),
    ("dashleftharpoondown", "⥫"),
    ("dashrightharpoondown", "⥭"),
    ("dashv", "⊣"),
    ("dbkarow", "⤏"),
    ("dblarrowupdown", "⇅"),
    ("ddagger", "‡"),
    ("ddddot", "⃜"),
    ("dddot", "⃛"),
    ("ddfnc", "⦙"),
    ("ddot", "̈"),
    ("ddots", "⋱"),
    ("ddotseq", "⩷"),
    ("defas", "⧋"),
    ("degree", "°"),
    ("del", "∇"),
    ("delta", "δ"),
    ("dh", "ð"),
    ("diagdown", "╲"),
    ("diagup", "╱"),
    ("diameter", "⌀"),
    ("diamond", "⋄"),
    ("diamondbotblack", "⬙"),
    ("diamondleftarrow", "⤝"),
    ("diamondleftarrowbar", "⤟"),
    ("diamondleftblack", "⬖"),
    ("diamondrightblack", "⬗"),
    ("diamondsuit", "♢"),
    ("diamondtopblack", "⬘"),
    ("dicei", "⚀"),
    ("diceii", "⚁"),
    ("diceiii", "⚂"),
    ("diceiv", "⚃"),
    ("dicev", "⚄"),
    ("dicevi", "⚅"),
    ("digamma", "ϝ"),
    ("dingasterisk", "✽"),
    ("disin", "⋲"),
    ("disjquant", "⨈"),
    ("div", "÷"),
    ("divideontimes", "⋇"),
    ("dj", "đ"),
    ("dlcorn", "⎣"),
    ("dot", "̇"),
    ("doteq", "≐"),
    ("dotequiv", "⩧"),
    ("dotminus", "∸"),
    ("dotplus", "∔"),
    ("dots", "…"),
    ("dotsim", "⩪"),
    ("dotsminusdots", "∺"),
    ("dottedcircle", "◌"),
    ("dottedsquare", "⬚"),
    ("dottimes", "⨰"),
    ("doublebarvee", "⩢"),
    ("doublepipe", "ǂ"),
    ("doubleplus", "⧺"),
    ("downarrow", "↓"),
    ("downarrowbarred", "⤈"),
    ("downdasharrow", "⇣"),
    ("downdownarrows", "⇊"),
    ("downharpoonleft", "⇃"),
    ("downharpoonright", "⇂"),
    ("downharpoonsleftright", "⥥"),
    ("downvDash", "⫪"),
    ("downwhitearrow", "⇩"),
    ("downzigzagarrow", "↯"),
    ("draftingarrow", "➛"),
    ("drbkarrow", "⤐"),
    ("droang", "̚"),
    ("dshfnc", "┆"),
    ("dsol", "⧶"),
    ("dualmap", "⧟"),
    ("dyogh", "ʤ"),
    ("egsdot", "⪘"),
    ("eighthnote", "♪"),
    ("elinters", "⏧"),
    ("ell", "ℓ"),
    ("elsdot", "⪗"),
    ("emdash", "—"),
    ("emptyset", "∅"),
    ("emptysetoarr", "⦳"),
    ("emptysetoarrl", "⦴"),
    ("emptysetobar", "⦱"),
    ("emptysetocirc", "⦲"),
    ("enclosecircle", "⃝"),
    ("enclosediamond", "⃟"),
    ("enclosesquare", "⃞"),
    ("enclosetriangle", "⃤"),
    ("endash", "–"),
    ("enspace", " "),
    ("eparsl", "⧣"),
    ("epsilon", "ϵ"),
    ("eqcirc", "≖"),
    ("eqcolon", "≕"),
    ("eqdef", "≝"),
    ("eqdot", "⩦"),
    ("eqeqeq", "⩶"),
    ("eqgtr", "⋝"),
    ("eqless", "⋜"),
    ("eqqgtr", "⪚"),
    ("eqqless", "⪙"),
    ("eqqplus", "⩱"),
    ("eqqsim", "⩳"),
    ("eqqslantgtr", "⪜"),
    ("eqqslantless", "⪛"),
    ("eqsim", "≂"),
    ("eqslantgtr", "⪖"),
    ("eqslantless", "⪕"),
    ("equalleftarrow", "⭀"),
    ("equalparallel", "⋕"),
    ("equiv", "≡"),
    ("equivDD", "⩸"),
    ("eqvparsl", "⧥"),
    ("esh", "ʃ"),
    ("eta", "η"),
    ("eth", "ð"),
    ("euler", "ℯ"),
    ("eulermascheroni", "ℇ"),
    ("euro", "€"),
    ("exclamdown", "¡"),
    ("exists", "∃"),
    ("fallingdotseq", "≒"),
    ("fdiagovnearrow", "⤯"),
    ("fdiagovrdiag", "⤬"),
    ("female", "♀"),
    ("fhr", "ɾ"),
    ("fisheye", "◉"),
    ("flat", "♭"),
    ("flatflat", "𝄫"),
    ("fltns", "⏥"),
    ("forall", "∀"),
    ("forks", "⫝̸"),
    ("forksnot", "⫝"),
    ("forkv", "⫙"),
    ("fourthroot", "∜"),
    ("frakA", "𝔄"),
    ("frakB", "𝔅"),
    ("frakC", "ℭ"),
    ("frakD", "𝔇"),
    ("frakE", "𝔈"),
    ("frakF", "𝔉"),
    ("frakG", "𝔊"),
    ("frakH", "ℌ"),
    ("frakI", "ℑ"),
    ("frakJ", "𝔍"),
    ("frakK", "𝔎"),
    ("frakL", "𝔏"),
    ("frakM", "𝔐"),
    ("frakN", "𝔑"),
    ("frakO", "𝔒"),
    ("frakP", "𝔓"),
    ("frakQ", "𝔔"),
    ("frakR", "ℜ"),
    ("frakS", "𝔖"),
    ("frakT", "𝔗"),
    ("frakU", "𝔘"),
    ("frakV", "𝔙"),
    ("frakW", "𝔚"),
    ("frakX", "𝔛"),
    ("frakY", "𝔜"),
    ("frakZ", "ℨ"),
    ("fraka", "𝔞"),
    ("frakb", "𝔟"),
    ("frakc", "𝔠"),
    ("frakd", "𝔡"),
    ("frake", "𝔢"),
    ("frakf", "𝔣"),
    ("frakg", "𝔤"),
    ("frakh", "𝔥"),
    ("fraki", "𝔦"),
    ("frakj", "𝔧"),
    ("frakk", "𝔨"),
    ("frakl", "𝔩"),
    ("frakm", "𝔪"),
    ("frakn", "𝔫"),
    ("frako", "𝔬"),
    ("frakp", "𝔭"),
    ("frakq", "𝔮"),
    ("frakr", "𝔯"),
    ("fraks", "𝔰"),
    ("frakt", "𝔱"),
    ("fraku", "𝔲"),
    ("frakv", "𝔳"),
    ("frakw", "𝔴"),
    ("frakx", "𝔵"),
    ("fraky", "𝔶"),
    ("frakz", "𝔷"),
    ("frown", "⌢"),
    ("fullouterjoin", "⟗"),
    ("gamma", "γ"),
    ("ge", "≥"),
    ("gemini", "♊"),
    ("geq", "≥"),
    ("geqq", "≧"),
    ("geqqslant", "⫺"),
    ("geqslant", "⩾"),
    ("gescc", "⪩"),
    ("gesdot", "⪀"),
    ("gesdoto", "⪂"),
    ("gesdotol", "⪄"),
    ("gesles", "⪔"),
    ("gg", "≫"),
    ("ggg", "⋙"),
    ("gggnest", "⫸"),
    ("gimel", "ℷ"),
    ("glE", "⪒"),
    ("gla", "⪥"),
    ("glj", "⪤"),
    ("glst", "ʔ"),
    ("gnapprox", "⪊"),
    ("gneq", "⪈"),
    ("gneqq", "≩"),
    ("gnsim", "⋧"),
    ("grave", "̀"),
    ("gsime", "⪎"),
    ("gsiml", "⪐"),
    ("gtcc", "⪧"),
    ("gtcir", "⩺"),
    ("gtquest", "⩼"),
    ("gtrapprox", "⪆"),
    ("gtrdot", "⋗"),
    ("gtreqless", "⋛"),
    ("gtreqqless", "⪌"),
    ("gtrless", "≷"),
    ("gtrsim", "≳"),
    ("guillemotleft", "«"),
    ("guillemotright", "»"),
    ("guilsinglleft", "‹"),
    ("guilsinglright", "›"),
    ("gvertneqq", "≩︀"),
    ("hat", "̂"),
    ("hatapprox", "⩯"),
    ("hbar", "ħ"),
    ("heartsuit", "♡"),
    ("hermaphrodite", "⚥"),
    ("hermitconjmatrix", "⊹"),
    ("hexagon", "⎔"),
    ("hexagonblack", "⬣"),
    ("highminus", "¯"),
    ("hksearow", "⤥"),
    ("hkswarow", "⤦"),
    ("hlmrk", "ˑ"),
    ("hookleftarrow", "↩"),
    ("hookrightarrow", "↪"),
    ("hookunderrightarrow", "🢲"),
    ("house", "⌂"),
    ("hrectangle", "▭"),
    ("hrectangleblack", "▬"),
    ("hslash", "ℏ"),
    ("hspace", " "),
    ("hvlig", "ƕ"),
    ("iff", "⟺"),
    ("iiiint", "⨌"),
    ("iiint", "∭"),
    ("iint", "∬"),
    ("image", "⊷"),
    ("imath", "ı"),
    ("impliedby", "⟸"),
    ("implies", "⟹"),
    ("in", "∈"),
    ("increment", "∆"),
    ("indep", "⫫"),
    ("infty", "∞"),
    ("inglst", "ʖ"),
    ("int", "∫"),
    ("intBar", "⨎"),
    ("intbar", "⨍"),
    ("intcap", "⨙"),
    ("intcup", "⨚"),
    ("intercal", "⊺"),
    ("interleave", "⫴"),
    ("intprod", "⨼"),
    ("intprodr", "⨽"),
    ("intx", "⨘"),
    ("inversewhitecircle", "◙"),
    ("invnot", "⌐"),
    ("invv", "ʌ"),
    ("invw", "ʍ"),
    ("invwhitelowerhalfcircle", "◛"),
    ("invwhiteupperhalfcircle", "◚"),
    ("iota", "ι"),
    ("isansA", "𝘈"),
    ("isansB", "𝘉"),
    ("isansC", "𝘊"),
    ("isansD", "𝘋"),
    ("isansE", "𝘌"),
    ("isansF", "𝘍"),
    ("isansG", "𝘎"),
    ("isansH", "𝘏"),
    ("isansI", "𝘐"),
    ("isansJ", "𝘑"),
    ("isansK", "𝘒"),
    ("isansL", "𝘓"),
    ("isansM", "𝘔"),
    ("isansN", "𝘕"),
    ("isansO", "𝘖"),
    ("isansP", "𝘗"),
    ("isansQ", "𝘘"),
    ("isansR", "𝘙"),
    ("isansS", "𝘚"),
    ("isansT", "𝘛"),
    ("isansU", "𝘜"),
    ("isansV", "𝘝"),
    ("isansW", "𝘞"),
    ("isansX", "𝘟"),
    ("isansY", "𝘠"),
    ("isansZ", "𝘡"),
    ("isansa", "𝘢"),
    ("isansb", "𝘣"),
    ("isansc", "𝘤"),
    ("isansd", "𝘥"),
    ("isanse", "𝘦"),
    ("isansf", "𝘧"),
    ("isansg", "𝘨"),
    ("isansh", "𝘩"),
    ("isansi", "𝘪"),
    ("isansj", "𝘫"),
    ("isansk", "𝘬"),
    ("isansl", "𝘭"),
    ("isansm", "𝘮"),
    ("isansn", "𝘯"),
    ("isanso", "𝘰"),
    ("isansp", "𝘱"),
    ("isansq", "𝘲"),
    ("isansr", "𝘳"),
    ("isanss", "𝘴"),
    ("isanst", "𝘵"),
    ("isansu", "𝘶"),
    ("isansv", "𝘷"),
    ("isansw", "𝘸"),
    ("isansx", "𝘹"),
    ("isansy", "𝘺"),
    ("isansz", "𝘻"),
    ("isinE", "⋹"),
    ("isindot", "⋵"),
    ("isinobar", "⋷"),
    ("isins", "⋴"),
    ("isinvb", "⋸"),
    ("itA", "𝐴"),
    ("itAlpha", "𝛢"),
    ("itB", "𝐵"),
    ("itBeta", "𝛣"),
    ("itC", "𝐶"),
    ("itChi", "𝛸"),
    ("itD", "𝐷"),
    ("itDelta", "𝛥"),
    ("itE", "𝐸"),
    ("itEpsilon", "𝛦"),
    ("itEta", "𝛨"),
    ("itF", "𝐹"),
    ("itG", "𝐺"),
    ("itGamma", "𝛤"),
    ("itH", "𝐻"),
    ("itI", "𝐼"),
    ("itIota", "𝛪"),
    ("itJ", "𝐽"),
    ("itK", "𝐾"),
    ("itKappa", "𝛫"),
    ("itL", "𝐿"),
    ("itLambda", "𝛬"),
    ("itM", "𝑀"),
    ("itMu", "𝛭"),
    ("itN", "𝑁"),
    ("itNu", "𝛮"),
    ("itO", "𝑂"),
    ("itOmega", "𝛺"),
    ("itOmicron", "𝛰"),
    ("itP", "𝑃"),
    ("itPhi", "𝛷"),
    ("itPi", "𝛱"),
    ("itPsi", "𝛹"),
    ("itQ", "𝑄"),
    ("itR", "𝑅"),
    ("itRho", "𝛲"),
    ("itS", "𝑆"),
    ("itSigma", "𝛴"),
    ("itT", "𝑇"),
    ("itTau", "𝛵"),
    ("itTheta", "𝛩"),
    ("itU", "𝑈"),
    ("itUpsilon", "𝛶"),
    ("itV", "𝑉"),
    ("itW", "𝑊"),
    ("itX", "𝑋"),
    ("itXi", "𝛯"),
    ("itY", "𝑌"),
    ("itZ", "𝑍"),
    ("itZeta", "𝛧"),
    ("ita", "𝑎"),
    ("italpha", "𝛼"),
    ("itb", "𝑏"),
    ("itbeta", "𝛽"),
    ("itc", "𝑐"),
    ("itchi", "𝜒"),
    ("itd", "𝑑"),
    ("itdelta", "𝛿"),
    ("ite", "𝑒"),
    ("itepsilon", "𝜖"),
    ("iteta", "𝜂"),
    ("itf", "𝑓"),
    ("itg", "𝑔"),
    ("itgamma", "𝛾"),
    ("ith", "ℎ"),
    ("iti", "𝑖"),
    ("itiota", "𝜄"),
    ("itj", "𝑗"),
    ("itk", "𝑘"),
    ("itkappa", "𝜅"),
    ("itl", "𝑙"),
    ("itlambda", "𝜆"),
    ("itm", "𝑚"),
    ("itmu", "𝜇"),
    ("itn", "𝑛"),
    ("itnabla", "𝛻"),
    ("itnu", "𝜈"),
    ("ito", "𝑜"),
    ("itomega", "𝜔"),
    ("itomicron", "𝜊"),
    ("itp", "𝑝"),
    ("itpartial", "𝜕"),
    ("itphi", "𝜙"),
    ("itpi", "𝜋"),
    ("itpsi", "𝜓"),
    ("itq", "𝑞"),
    ("itr", "𝑟"),
    ("itrho", "𝜌"),
    ("its", "𝑠"),
    ("itsigma", "𝜎"),
    ("itt", "𝑡"),
    ("ittau", "𝜏"),
    ("ittheta", "𝜃"),
    ("itu", "𝑢"),
    ("itupsilon", "𝜐"),
    ("itv", "𝑣"),
    ("itvarTheta", "𝛳"),
    ("itvarepsilon", "𝜀"),
    ("itvarkappa", "𝜘"),
    ("itvarphi", "𝜑"),
    ("itvarpi", "𝜛"),
    ("itvarrho", "𝜚"),
    ("itvarsigma", "𝜍"),
    ("itvartheta", "𝜗"),
    ("itw", "𝑤"),
    ("itx", "𝑥"),
    ("itxi", "𝜉"),
    ("ity", "𝑦"),
    ("itz", "𝑧"),
    ("itzeta", "𝜁"),
    ("jmath", "ȷ"),
    ("join", "⨝"),
    ("jupiter", "♃"),
    ("k", "̨"),
    ("kappa", "κ"),
    ("kernelcontraction", "∻"),
    ("koppa", "ϟ"),
    ("l", "ł"),
    ("lambda", "λ"),
    ("langle", "⟨"),
    ("lat", "⪫"),
    ("late", "⪭"),
    ("lazysinv", "∾"),
    ("lceil", "⌈"),
    ("ldots", "…"),
    ("ldq", "“"),
    ("le", "≤"),
    ("leftarrow", "←"),
    ("leftarrowapprox", "⭊"),
    ("leftarrowbackapprox", "⭂"),
    ("leftarrowbsimilar", "⭋"),
    ("leftarrowless", "⥷"),
    ("leftarrowonoplus", "⬲"),
    ("leftarrowplus", "⥆"),
    ("leftarrowsubset", "⥺"),
    ("leftarrowtail", "↢"),
    ("leftarrowtriangle", "⇽"),
    ("leftarrowx", "⬾"),
    ("leftbkarrow", "⤌"),
    ("leftcurvedarrow", "⬿"),
    ("leftdasharrow", "⇠"),
    ("leftdbkarrow", "⤎"),
    ("leftdotarrow", "⬸"),
    ("leftharpoonaccent", "⃐"),
    ("leftharpoondown", "↽"),
    ("leftharpoonsupdown", "⥢"),
    ("leftharpoonup", "↼"),
    ("leftharpoonupdash", "⥪"),
    ("leftleftarrows", "⇇"),
    ("leftmoon", "☾"),
    ("leftouterjoin", "⟕"),
    ("leftrepeatsign", "𝄆"),
    ("leftrightarrow", "↔"),
    ("leftrightarrowcircle", "⥈"),
    ("leftrightarrows", "⇆"),
    ("leftrightarrowtriangle", "⇿"),
    ("leftrightharpoondownup", "⥋"),
    ("leftrightharpoons", "⇋"),
    ("leftrightharpoonsdown", "⥧"),
    ("leftrightharpoonsup", "⥦"),
    ("leftrightharpoonupdown", "⥊"),
    ("leftrightsquigarrow", "↭"),
    ("leftsquigarrow", "⇜"),
    ("leftthreearrows", "⬱"),
    ("leftthreetimes", "⋋"),
    ("leftwavearrow", "↜"),
    ("leftwhitearrow", "⇦"),
    ("leo", "♌"),
    ("leq", "≤"),
    ("leqq", "≦"),
    ("leqqslant", "⫹"),
    ("leqslant", "⩽"),
    ("lescc", "⪨"),
    ("lesdot", "⩿"),
    ("lesdoto", "⪁"),
    ("lesdotor", "⪃"),
    ("lesges", "⪓"),
    ("lessapprox", "⪅"),
    ("lessdot", "⋖"),
    ("lesseqgtr", "⋚"),
    ("lesseqqgtr", "⪋"),
    ("lessgtr", "≶"),
    ("lesssim", "≲"),
    ("lfloor", "⌊"),
    ("lgE", "⪑"),
    ("lgblkcircle", "⬤"),
    ("lgblksquare", "⬛"),
    ("lgwhtcircle", "◯"),
    ("lgwhtsquare", "⬜"),
    ("libra", "♎"),
    ("linefeed", "↴"),
    ("ll", "≪"),
    ("llarc", "◟"),
    ("llblacktriangle", "◣"),
    ("llbracket", "⟦"),
    ("llcorner", "⌞"),
    ("lllnest", "⫷"),
    ("lltriangle", "◺"),
    ("lmoustache", "⎰"),
    ("lmrk", "ː"),
    ("lnapprox", "⪉"),
    ("lneq", "⪇"),
    ("lneqq", "≨"),
    ("lnsim", "⋦"),
    ("longleftarrow", "⟵"),
    ("longleftrightarrow", "⟷"),
    ("longleftsquigarrow", "⬳"),
    ("longmapsfrom", "⟻"),
    ("longmapsto", "⟼"),
    ("longrightarrow", "⟶"),
    ("longrightsquigarrow", "⟿"),
    ("looparrowleft", "↫"),
    ("looparrowright", "↬"),
    ("low", "˕"),
    ("lowint", "⨜"),
    ("lozenge", "◊"),
    ("lpargt", "⦠"),
    ("lq", "‘"),
    ("lrarc", "◞"),
    ("lrblacktriangle", "◢"),
    ("lrcorner", "⌟"),
    ("lrtriangle", "◿"),
    ("lrtriangleeq", "⧡"),
    ("lsime", "⪍"),
    ("lsimg", "⪏"),
    ("lsqhook", "⫍"),
    ("ltcc", "⪦"),
    ("ltcir", "⩹"),
    ("ltimes", "⋉"),
    ("ltlmr", "ɱ"),
    ("ltln", "ɲ"),
    ("ltphi", "ɸ"),
    ("ltquest", "⩻"),
    ("lvboxline", "⎸"),
    ("lvertneqq", "≨︀"),
    ("male", "♂"),
    ("maltese", "✠"),
    ("mapsdown", "↧"),
    ("mapsfrom", "↤"),
    ("mapsto", "↦"),
    ("mapsup", "↥"),
    ("mars", "♂"),
    ("mdblkcircle", "⚫"),
    ("mdblkdiamond", "⬥"),
    ("mdblklozenge", "⬧"),
    ("mdblksquare", "◼"),
    ("mdlgblkcircle", "●"),
    ("mdlgblkdiamond", "◆"),
    ("mdlgwhtdiamond", "◇"),
    ("mdsmblksquare", "◾"),
    ("mdsmwhtcircle", "⚬"),
    ("mdsmwhtsquare", "◽"),
    ("mdwhtcircle", "⚪"),
    ("mdwhtdiamond", "⬦"),
    ("mdwhtlozenge", "⬨"),
    ("mdwhtsquare", "◻"),
    ("measangledltosw", "⦯"),
    ("measangledrtose", "⦮"),
    ("measangleldtosw", "⦫"),
    ("measanglelutonw", "⦩"),
    ("measanglerdtose", "⦪"),
    ("measanglerutone", "⦨"),
    ("measangleultonw", "⦭"),
    ("measangleurtone", "⦬"),
    ("measeq", "≞"),
    ("measuredangle", "∡"),
    ("measuredangleleft", "⦛"),
    ("medblackstar", "⭑"),
    ("medwhitestar", "⭐"),
    ("mercury", "☿"),
    ("mho", "℧"),
    ("mid", "∣"),
    ("midbarvee", "⩝"),
    ("midbarwedge", "⩜"),
    ("minhat", "⩟"),
    ("minus", "−"),
    ("minusdot", "⨪"),
    ("minusfdots", "⨫"),
    ("minusrdots", "⨬"),
    ("mlcp", "⫛"),
    ("models", "⊧"),
    ("modtwosum", "⨊"),
    ("mp", "∓"),
    ("mu", "μ"),
    ("multimap", "⊸"),
    ("nBumpeq", "≎̸"),
    ("nHdownarrow", "⇟"),
    ("nHuparrow", "⇞"),
    ("nLeftarrow", "⇍"),
    ("nLeftrightarrow", "⇎"),
    ("nRightarrow", "⇏"),
    ("nVDash", "⊯"),
    ("nVdash", "⊮"),
    ("nVleftarrow", "⇺"),
    ("nVleftarrowtail", "⬺"),
    ("nVleftrightarrow", "⇼"),
    ("nVrightarrow", "⇻"),
    ("nVrightarrowtail", "⤕"),
    ("nVtwoheadleftarrow", "⬵"),
    ("nVtwoheadleftarrowtail", "⬽"),
    ("nVtwoheadrightarrow", "⤁"),
    ("nVtwoheadrightarrowtail", "⤘"),
    ("nabla", "∇"),
    ("nand", "⊼"),
    ("napprox", "≉"),
    ("nasymp", "≭"),
    ("natural", "♮"),
    ("nbumpeq", "≏̸"),
    ("ncong", "≇"),
    ("ne", "≠"),
    ("nearrow", "↗"),
    ("neg", "¬"),
    ("neovnwarrow", "⤱"),
    ("neovsearrow", "⤮"),
    ("neptune", "♆"),
    ("neq", "≠"),
    ("neqsim", "≂̸"),
    ("nequiv", "≢"),
    ("neuter", "⚲"),
    ("nexists", "∄"),
    ("ng", "ŋ"),
    ("ngeq", "≱"),
    ("ngeqslant", "⩾̸"),
    ("ngtr", "≯"),
    ("ngtrsim", "≵"),
    ("ni", "∋"),
    ("niobar", "⋾"),
    ("nis", "⋼"),
    ("nisd", "⋺"),
    ("nleftarrow", "↚"),
    ("nleftrightarrow", "↮"),
    ("nleq", "≰"),
    ("nleqslant", "⩽̸"),
    ("nless", "≮"),
    ("nlesssim", "≴"),
    ("nmid", "∤"),
    ("nni", "∌"),
    ("nolinebreak", "⁠"),
    ("nor", "⊽"),
    ("not", "̸"),
    ("notbackslash", "⍀"),
    ("note128th", "𝅘𝅥𝅲"),
    ("note16th", "𝅘𝅥𝅯"),
    ("note32th", "𝅘𝅥𝅰"),
    ("note64th", "𝅘𝅥𝅱"),
    ("note8th", "𝅘𝅥𝅮"),
    ("notedoublewhole", "𝅜"),
    ("notehalf", "𝅗𝅥"),
    ("notequarter", "𝅘𝅥"),
    ("notewhole", "𝅝"),
    ("notgreaterless", "≹"),
    ("notin", "∉"),
    ("notlessgreater", "≸"),
    ("notslash", "⌿"),
    ("nparallel", "∦"),
    ("npolint", "⨔"),
    ("nprec", "⊀"),
    ("npreccurlyeq", "⋠"),
    ("npreceq", "⪯̸"),
    ("nprecsim", "≾̸"),
    ("nrightarrow", "↛"),
    ("nrleg", "ƞ"),
    ("nsim", "≁"),
    ("nsime", "≄"),
    ("nsqsubseteq", "⋢"),
    ("nsqsupseteq", "⋣"),
    ("nsubset", "⊄"),
    ("nsubseteq", "⊈"),
    ("nsubseteqq", "⫅̸"),
    ("nsucc", "⊁"),
    ("nsucccurlyeq", "⋡"),
    ("nsucceq", "⪰̸"),
    ("nsuccsim", "≿̸"),
    ("nsupset", "⊅"),
    ("nsupseteq", "⊉"),
    ("nsupseteqq", "⫆̸"),
    ("ntriangleleft", "⋪"),
    ("ntrianglelefteq", "⋬"),
    ("ntriangleright", "⋫"),
    ("ntrianglerighteq", "⋭"),
    ("nu", "ν"),
    ("numero", "№"),
    ("nvDash", "⊭"),
    ("nvLeftarrow", "⤂"),
    ("nvLeftrightarrow", "⤄"),
    ("nvRightarrow", "⤃"),
    ("nvdash", "⊬"),
    ("nvleftarrow", "⇷"),
    ("nvleftarrowtail", "⬹"),
    ("nvleftrightarrow", "⇹"),
    ("nvrightarrow", "⇸"),
    ("nvrightarrowtail", "⤔"),
    ("nvtwoheadleftarrow", "⬴"),
    ("nvtwoheadleftarrowtail", "⬼"),
    ("nvtwoheadrightarrow", "⤀"),
    ("nvtwoheadrightarrowtail", "⤗"),
    ("nwarrow", "↖"),
    ("nwovnearrow", "⤲"),
    ("o", "ø"),
    ("obar", "⌽"),
    ("obslash", "⦸"),
    ("ocirc", "̊"),
    ("ocommatopright", "̕"),
    ("odiv", "⨸"),
    ("odot", "⊙"),
    ("odotslashdot", "⦼"),
    ("oe", "œ"),
    ("ogreaterthan", "⧁"),
    ("ohm", "Ω"),
    ("oiiint", "∰"),
    ("oiint", "∯"),
    ("oint", "∮"),
    ("ointctrclockwise", "∳"),
    ("oldKoppa", "Ϙ"),
    ("oldkoppa", "ϙ"),
    ("olessthan", "⧀"),
    ("omega", "ω"),
    ("omicron", "ο"),
    ("ominus", "⊖"),
    ("openbracketleft", "⟦"),
    ("openbracketright", "⟧"),
    ("openo", "ɔ"),
    ("oplus", "⊕"),
    ("opluslhrim", "⨭"),
    ("oplusrhrim", "⨮"),
    ("ordfeminine", "ª"),
    ("ordmasculine", "º"),
    ("original", "⊶"),
    ("oslash", "⊘"),
    ("otimes", "⊗"),
    ("otimeshat", "⨶"),
    ("otimeslhrim", "⨴"),
    ("otimesrhrim", "⨵"),
    ("oturnedcomma", "̒"),
    ("overbar", "̅"),
    ("overbrace", "⏞"),
    ("overbracket", "⎴"),
    ("overleftarrow", "⃖"),
    ("overleftrightarrow", "⃡"),
    ("ovhook", "̉"),
    ("palh", "̡"),
    ("parallel", "∥"),
    ("parallelogram", "▱"),
    ("parallelogramblack", "▰"),
    ("partial", "∂"),
    ("partialmeetcontraction", "⪣"),
    ("pbgam", "ɤ"),
    ("pentagon", "⬠"),
    ("pentagonblack", "⬟"),
    ("perp", "⟂"),
    ("perspcorrespond", "⩞"),
    ("pertenthousand", "‱"),
    ("perthousand", "‰"),
    ("pes", "₧"),
    ("pgamma", "ɣ"),
    ("phi", "ϕ"),
    ("pi", "π"),
    ("pisces", "♓"),
    ("pitchfork", "⋔"),
    ("planck", "ℎ"),
    ("plusdot", "⨥"),
    ("pluseqq", "⩲"),
    ("plushat", "⨣"),
    ("plussim", "⨦"),
    ("plussubtwo", "⨧"),
    ("plustrif", "⨨"),
    ("pluto", "♇"),
    ("pm", "±"),
    ("pointint", "⨕"),
    ("postalmark", "〒"),
    ("pppprime", "⁗"),
    ("ppprime", "‴"),
    ("pprime", "″"),
    ("prec", "≺"),
    ("precapprox", "⪷"),
    ("preccurlyeq", "≼"),
    ("preceq", "⪯"),
    ("preceqq", "⪳"),
    ("precnapprox", "⪹"),
    ("precneq", "⪱"),
    ("precneqq", "⪵"),
    ("precnsim", "⋨"),
    ("precsim", "≾"),
    ("prime", "′"),
    ("prod", "∏"),
    ("profline", "⌒"),
    ("profsurf", "⌓"),
    ("propto", "∝"),
    ("prurel", "⊰"),
    ("pscrv", "ʋ"),
    ("psi", "ψ"),
    ("pupsil", "ʊ"),
    ("quad", " "),
    ("quarternote", "♩"),
    ("questeq", "≟"),
    ("questiondown", "¿"),
    ("rLarr", "⥄"),
    ("rais", "˔"),
    ("rangle", "⟩"),
    ("rarrx", "⥇"),
    ("rasp", "ʼ"),
    ("rceil", "⌉"),
    ("rdiagovfdiag", "⤫"),
    ("rdiagovsearrow", "⤰"),
    ("rdq", "”"),
    ("reapos", "‛"),
    ("recorder", "⌕"),
    ("reglst", "ʕ"),
    ("rest128th", "𝅂"),
    ("rest16th", "𝄿"),
    ("rest32th", "𝅀"),
    ("rest64th", "𝅁"),
    ("rest8th", "𝄾"),
    ("resthalf", "𝄼"),
    ("restmulti", "𝄺"),
    ("restquarter", "𝄽"),
    ("restwhole", "𝄻"),
    ("revangle", "⦣"),
    ("revangleubar", "⦥"),
    ("revemptyset", "⦰"),
    ("rfloor", "⌋"),
    ("rh", "̢"),
    ("rho", "ρ"),
    ("rightangle", "∟"),
    ("rightanglearc", "⊾"),
    ("rightanglemdot", "⦝"),
    ("rightarrow", "→"),
    ("rightarrowbackapprox", "⭈"),
    ("rightarrowbar", "⇥"),
    ("rightarrowbsimilar", "⭌"),
    ("rightarrowdiamond", "⤞"),
    ("rightarrowgtr", "⭃"),
    ("rightarrowplus", "⥅"),
    ("rightarrowsupset", "⭄"),
    ("rightarrowtail", "↣"),
    ("rightarrowtriangle", "⇾"),
    ("rightdasharrow", "⇢"),
    ("rightdotarrow", "⤑"),
    ("rightharpoonaccent", "⃑"),
    ("rightharpoondown", "⇁"),
    ("rightharpoonsupdown", "⥤"),
    ("rightharpoonup", "⇀"),
    ("rightharpoonupdash", "⥬"),
    ("rightleftarrows", "⇄"),
    ("rightleftharpoons", "⇌"),
    ("rightleftharpoonsdown", "⥩"),
    ("rightleftharpoonsup", "⥨"),
    ("rightmoon", "☽"),
    ("rightouterjoin", "⟖"),
    ("rightpentagon", "⭔"),
    ("rightpentagonblack", "⭓"),
    ("rightrepeatsign", "𝄇"),
    ("rightrightarrows", "⇉"),
    ("rightsquigarrow", "⇝"),
    ("rightthreearrows", "⇶"),
    ("rightthreetimes", "⋌"),
    ("rightwavearrow", "↝"),
    ("rightwhitearrow", "⇨"),
    ("ringplus", "⨢"),
    ("risingdotseq", "≓"),
    ("rl", "ɼ"),
    ("rmoustache", "⎱"),
    ("rppolint", "⨒"),
    ("rq", "’"),
    ("rrbracket", "⟧"),
    ("rsolbar", "⧷"),
    ("rsqhook", "⫎"),
    ("rtimes", "⋊"),
    ("rtld", "ɖ"),
    ("rtll", "ɭ"),
    ("rtln", "ɳ"),
    ("rtlr", "ɽ"),
    ("rtls", "ʂ"),
    ("rtlt", "ʈ"),
    ("rtlz", "ʐ"),
    ("rttrnr", "ɻ"),
    ("rvboxline", "⎹"),
    ("rvbull", "◘"),
    ("sagittarius", "♐"),
    ("sampi", "ϡ"),
    ("sansA", "𝖠"),
    ("sansB", "𝖡"),
    ("sansC", "𝖢"),
    ("sansD", "𝖣"),
    ("sansE", "𝖤"),
    ("sansF", "𝖥"),
    ("sansG", "𝖦"),
    ("sansH", "𝖧"),
    ("sansI", "𝖨"),
    ("sansJ", "𝖩"),
    ("sansK", "𝖪"),
    ("sansL", "𝖫"),
    ("sansLmirrored", "⅃"),
    ("sansLturned", "⅂"),
    ("sansM", "𝖬"),
    ("sansN", "𝖭"),
    ("sansO", "𝖮"),
    ("sansP", "𝖯"),
    ("sansQ", "𝖰"),
    ("sansR", "𝖱"),
    ("sansS", "𝖲"),
    ("sansT", "𝖳"),
    ("sansU", "𝖴"),
    ("sansV", "𝖵"),
    ("sansW", "𝖶"),
    ("sansX", "𝖷"),
    ("sansY", "𝖸"),
    ("sansZ", "𝖹"),
    ("sansa", "𝖺"),
    ("sansb", "𝖻"),
    ("sansc", "𝖼"),
    ("sansd", "𝖽"),
    ("sanse", "𝖾"),
    ("sanseight", "𝟪"),
    ("sansf", "𝖿"),
    ("sansfive", "𝟧"),
    ("sansfour", "𝟦"),
    ("sansg", "𝗀"),
    ("sansh", "𝗁"),
    ("sansi", "𝗂"),
    ("sansj", "𝗃"),
    ("sansk", "𝗄"),
    ("sansl", "𝗅"),
    ("sansm", "𝗆"),
    ("sansn", "𝗇"),
    ("sansnine", "𝟫"),
    ("sanso", "𝗈"),
    ("sansone", "𝟣"),
    ("sansp", "𝗉"),
    ("sansq", "𝗊"),
    ("sansr", "𝗋"),
    ("sanss", "𝗌"),
    ("sansseven", "𝟩"),
    ("sanssix", "𝟨"),
    ("sanst", "𝗍"),
    ("sansthree", "𝟥"),
    ("sanstwo", "𝟤"),
    ("sansu", "𝗎"),
    ("sansv", "𝗏"),
    ("sansw", "𝗐"),
    ("sansx", "𝗑"),
    ("sansy", "𝗒"),
    ("sansz", "𝗓"),
    ("sanszero", "𝟢"),
    ("saturn", "♄"),
    ("sbbrg", "̪"),
    ("sblhr", "˓"),
    ("sbrhr", "˒"),
    ("schwa", "ə"),
    ("scorpio", "♏"),
    ("scpolint", "⨓"),
    ("scrA", "𝒜"),
    ("scrB", "ℬ"),
    ("scrC", "𝒞"),
    ("scrD", "𝒟"),
    ("scrE", "ℰ"),
    ("scrF", "ℱ"),
    ("scrG", "𝒢"),
    ("scrH", "ℋ"),
    ("scrI", "ℐ"),
    ("scrJ", "𝒥"),
    ("scrK", "𝒦"),
    ("scrL", "ℒ"),
    ("scrM", "ℳ"),
    ("scrN", "𝒩"),
    ("scrO", "𝒪"),
    ("scrP", "𝒫"),
    ("scrQ", "𝒬"),
    ("scrR", "ℛ"),
    ("scrS", "𝒮"),
    ("scrT", "𝒯"),
    ("scrU", "𝒰"),
    ("scrV", "𝒱"),
    ("scrW", "𝒲"),
    ("scrX", "𝒳"),
    ("scrY", "𝒴"),
    ("scrZ", "𝒵"),
    ("scra", "𝒶"),
    ("scrb", "𝒷"),
    ("scrc", "𝒸"),
    ("scrd", "𝒹"),
    ("scre", "ℯ"),
    ("scrf", "𝒻"),
    ("scrg", "ℊ"),
    ("scrh", "𝒽"),
    ("scri", "𝒾"),
    ("scrj", "𝒿"),
    ("scrk", "𝓀"),
    ("scrm", "𝓂"),
    ("scrn", "𝓃"),
    ("scro", "ℴ"),
    ("scrp", "𝓅"),
    ("scrq", "𝓆"),
    ("scrr", "𝓇"),
    ("scrs", "𝓈"),
    ("scrt", "𝓉"),
    ("scru", "𝓊"),
    ("scrv", "𝓋"),
    ("scrw", "𝓌"),
    ("scrx", "𝓍"),
    ("scry", "𝓎"),
    ("scrz", "𝓏"),
    ("scurel", "⊱"),
    ("searrow", "↘"),
    ("segno", "𝄋"),
    ("seovnearrow", "⤭"),
    ("setminus", "∖"),
    ("sharp", "♯"),
    ("sharpsharp", "𝄪"),
    ("shuffle", "⧢"),
    ("sigma", "σ"),
    ("sim", "∼"),
    ("simeq", "≃"),
    ("simgE", "⪠"),
    ("simgtr", "⪞"),
    ("similarleftarrow", "⭉"),
    ("simlE", "⪟"),
    ("simless", "⪝"),
    ("simminussim", "⩬"),
    ("simplus", "⨤"),
    ("simrdots", "⩫"),
    ("sinewave", "∿"),
    ("smallblacktriangleleft", "◂"),
    ("smallblacktriangleright", "▸"),
    ("smallin", "∊"),
    ("smallni", "∍"),
    ("smalltriangleleft", "◃"),
    ("smalltriangleright", "▹"),
    ("smashtimes", "⨳"),
    ("smblkdiamond", "⬩"),
    ("smblklozenge", "⬪"),
    ("smblksquare", "▪"),
    ("smeparsl", "⧤"),
    ("smile", "⌣"),
    ("smt", "⪪"),
    ("smte", "⪬"),
    ("smwhitestar", "⭒"),
    ("smwhtcircle", "◦"),
    ("smwhtlozenge", "⬫"),
    ("smwhtsquare", "▫"),
    ("sout", "̶"),
    ("spadesuit", "♠"),
    ("sphericalangle", "∢"),
    ("sphericalangleup", "⦡"),
    ("sqcap", "⊓"),
    ("sqcup", "⊔"),
    ("sqfl", "◧"),
    ("sqfnw", "┙"),
    ("sqfr", "◨"),
    ("sqfse", "◪"),
    ("sqlozenge", "⌑"),
    ("sqrint", "⨖"),
    ("sqrt", "√"),
    ("sqrtbottom", "⎷"),
    ("sqsubset", "⊏"),
    ("sqsubseteq", "⊑"),
    ("sqsubsetneq", "⋤"),
    ("sqsupset", "⊐"),
    ("sqsupseteq", "⊒"),
    ("sqsupsetneq", "⋥"),
    ("square", "□"),
    ("squarebotblack", "⬓"),
    ("squarecrossfill", "▩"),
    ("squarehfill", "▤"),
    ("squarehvfill", "▦"),
    ("squarellblack", "⬕"),
    ("squarellquad", "◱"),
    ("squarelrquad", "◲"),
    ("squareneswfill", "▨"),
    ("squarenwsefill", "▧"),
    ("squaretopblack", "⬒"),
    ("squareulblack", "◩"),
    ("squareulquad", "◰"),
    ("squareurblack", "⬔"),
    ("squareurquad", "◳"),
    ("squarevfill", "▥"),
    ("squoval", "▢"),
    ("ss", "ß"),
    ("star", "⋆"),
    ("starequal", "≛"),
    ("sterling", "£"),
    ("stigma", "ϛ"),
    ("strike", "̶"),
    ("strns", "⏤"),
    ("subedot", "⫃"),
    ("submult", "⫁"),
    ("subset", "⊂"),
    ("subsetapprox", "⫉"),
    ("subsetdot", "⪽"),
    ("subseteq", "⊆"),
    ("subseteqq", "⫅"),
    ("subsetneq", "⊊"),
    ("subsetneqq", "⫋"),
    ("subsetplus", "⪿"),
    ("subsim", "⫇"),
    ("subsub", "⫕"),
    ("subsup", "⫓"),
    ("succ", "≻"),
    ("succapprox", "⪸"),
    ("succcurlyeq", "≽"),
    ("succeq", "⪰"),
    ("succeqq", "⪴"),
    ("succnapprox", "⪺"),
    ("succneq", "⪲"),
    ("succneqq", "⪶"),
    ("succnsim", "⋩"),
    ("succsim", "≿"),
    ("sum", "∑"),
    ("sumint", "⨋"),
    ("sun", "☼"),
    ("supdsub", "⫘"),
    ("supedot", "⫄"),
    ("suphsol", "⟉"),
    ("suphsub", "⫗"),
    ("supmult", "⫂"),
    ("supset", "⊃"),
    ("supsetapprox", "⫊"),
    ("supsetdot", "⪾"),
    ("supseteq", "⊇"),
    ("supseteqq", "⫆"),
    ("supsetneq", "⊋"),
    ("supsetneqq", "⫌"),
    ("supsetplus", "⫀"),
    ("supsim", "⫈"),
    ("supsub", "⫔"),
    ("supsup", "⫖"),
    ("surd", "√"),
    ("swarrow", "↙"),
    ("tau", "τ"),
    ("taurus", "♉"),
    ("tdcol", "⫶"),
    ("tesh", "ʧ"),
    ("th", "þ"),
    ("therefore", "∴"),
    ("theta", "θ"),
    ("thickspace", " "),
    ("thinspace", " "),
    ("threedangle", "⟀"),
    ("threeunderdot", "⃨"),
    ("tieconcat", "⁀"),
    ("tilde", "̃"),
    ("tildelow", "˜"),
    ("tildetrpl", "≋"),
    ("times", "×"),
    ("timesbar", "⨱"),
    ("to", "→"),
    ("toea", "⤨"),
    ("tona", "⤧"),
    ("top", "⊤"),
    ("topbot", "⌶"),
    ("topsemicircle", "◠"),
    ("tosa", "⤩"),
    ("towa", "⤪"),
    ("trademark", "™"),
    ("trapezium", "⏢"),
    ("trianglecdot", "◬"),
    ("triangledown", "▿"),
    ("triangleleft", "◁"),
    ("triangleleftblack", "◭"),
    ("trianglelefteq", "⊴"),
    ("triangleminus", "⨺"),
    ("triangleplus", "⨹"),
    ("triangleq", "≜"),
    ("triangleright", "▷"),
    ("trianglerightblack", "◮"),
    ("trianglerighteq", "⊵"),
    ("triangletimes", "⨻"),
    ("tricolon", "⁝"),
    ("tripleplus", "⧻"),
    ("trna", "ɐ"),
    ("trnh", "ɥ"),
    ("trnm", "ɯ"),
    ("trnmlr", "ɰ"),
    ("trnr", "ɹ"),
    ("trnrl", "ɺ"),
    ("trnsa", "ɒ"),
    ("trnt", "ʇ"),
    ("trny", "ʎ"),
    ("ttA", "𝙰"),
    ("ttB", "𝙱"),
    ("ttC", "𝙲"),
    ("ttD", "𝙳"),
    ("ttE", "𝙴"),
    ("ttF", "𝙵"),
    ("ttG", "𝙶"),
    ("ttH", "𝙷"),
    ("ttI", "𝙸"),
    ("ttJ", "𝙹"),
    ("ttK", "𝙺"),
    ("ttL", "𝙻"),
    ("ttM", "𝙼"),
    ("ttN", "𝙽"),
    ("ttO", "𝙾"),
    ("ttP", "𝙿"),
    ("ttQ", "𝚀"),
    ("ttR", "𝚁"),
    ("ttS", "𝚂"),
    ("ttT", "𝚃"),
    ("ttU", "𝚄"),
    ("ttV", "𝚅"),
    ("ttW", "𝚆"),
    ("ttX", "𝚇"),
    ("ttY", "𝚈"),
    ("ttZ", "𝚉"),
    ("tta", "𝚊"),
    ("ttb", "𝚋"),
    ("ttc", "𝚌"),
    ("ttd", "𝚍"),
    ("tte", "𝚎"),
    ("tteight", "𝟾"),
    ("ttf", "𝚏"),
    ("ttfive", "𝟻"),
    ("ttfour", "𝟺"),
    ("ttg", "𝚐"),
    ("tth", "𝚑"),
    ("tti", "𝚒"),
    ("ttj", "𝚓"),
    ("ttk", "𝚔"),
    ("ttl", "𝚕"),
    ("ttm", "𝚖"),
    ("ttn", "𝚗"),
    ("ttnine", "𝟿"),
    ("tto", "𝚘"),
    ("ttone", "𝟷"),
    ("ttp", "𝚙"),
    ("ttq", "𝚚"),
    ("ttr", "𝚛"),
    ("tts", "𝚜"),
    ("ttseven", "𝟽"),
    ("ttsix", "𝟼"),
    ("ttt", "𝚝"),
    ("ttthree", "𝟹"),
    ("tttwo", "𝟸"),
    ("ttu", "𝚞"),
    ("ttv", "𝚟"),
    ("ttw", "𝚠"),
    ("ttx", "𝚡"),
    ("tty", "𝚢"),
    ("ttz", "𝚣"),
    ("ttzero", "𝟶"),
    ("turnangle", "⦢"),
    ("turnediota", "℩"),
    ("turnednot", "⌙"),
    ("turnk", "ʞ"),
    ("twocaps", "⩋"),
    ("twocups", "⩊"),
    ("twoheaddownarrow", "↡"),
    ("twoheadleftarrow", "↞"),
    ("twoheadleftarrowtail", "⬻"),
    ("twoheadleftdbkarrow", "⬷"),
    ("twoheadmapsfrom", "⬶"),
    ("twoheadmapsto", "⤅"),
    ("twoheadrightarrow", "↠"),
    ("twoheadrightarrowtail", "⤖"),
    ("twoheaduparrow", "↟"),
    ("twoheaduparrowcircle", "⥉"),
    ("twonotes", "♫"),
    ("u", "˘"),
    ("ularc", "◜"),
    ("ulblacktriangle", "◤"),
    ("ulcorner", "⌜"),
    ("ultriangle", "◸"),
    ("uminus", "⩁"),
    ("underbar", "̲"),
    ("underbrace", "⏟"),
    ("underbracket", "⎵"),
    ("underleftarrow", "⃮"),
    ("underleftharpoondown", "⃭"),
    ("underleftrightarrow", "͍"),
    ("underrightarrow", "⃯"),
    ("underrightharpoondown", "⃬"),
    ("upand", "⅋"),
    ("uparrow", "↑"),
    ("uparrowbarred", "⤉"),
    ("updasharrow", "⇡"),
    ("updownarrow", "↕"),
    ("updownarrowbar", "↨"),
    ("updownharpoonleftright", "⥍"),
    ("updownharpoonrightleft", "⥌"),
    ("upharpoonleft", "↿"),
    ("upharpoonright", "↾"),
    ("upharpoonsleftright", "⥣"),
    ("upin", "⟒"),
    ("upint", "⨛"),
    ("uplus", "⊎"),
    ("upsilon", "υ"),
    ("upuparrows", "⇈"),
    ("upvDash", "⫫"),
    ("upwhitearrow", "⇧"),
    ("uranus", "♅"),
    ("urarc", "◝"),
    ("urblacktriangle", "◥"),
    ("urcorner", "⌝"),
    ("urtriangle", "◹"),
    ("vDash", "⊨"),
    ("varTheta", "ϴ"),
    ("varbarwedge", "⌅"),
    ("varbeta", "ϐ"),
    ("varcarriagereturn", "⏎"),
    ("varclubsuit", "♧"),
    ("vardiamondsuit", "♦"),
    ("vardoublebarwedge", "⌆"),
    ("varepsilon", "ε"),
    ("varheartsuit", "♥"),
    ("varhexagon", "⬡"),
    ("varhexagonblack", "⬢"),
    ("varhexagonlrbonds", "⌬"),
    ("varisinobar", "⋶"),
    ("varisins", "⋳"),
    ("varkappa", "ϰ"),
    ("varlrtriangle", "⊿"),
    ("varniobar", "⋽"),
    ("varnis", "⋻"),
    ("varnothing", "∅"),
    ("varointclockwise", "∲"),
    ("varphi", "φ"),
    ("varpi", "ϖ"),
    ("varrho", "ϱ"),
    ("varsigma", "ς"),
    ("varspadesuit", "♤"),
    ("varstar", "✶"),
    ("varsubsetneqq", "⊊︀"),
    ("varsupsetneq", "⊋︀"),
    ("vartheta", "ϑ"),
    ("vartriangle", "▵"),
    ("vartriangleleft", "⊲"),
    ("vartriangleright", "⊳"),
    ("varveebar", "⩡"),
    ("vdash", "⊢"),
    ("vdots", "⋮"),
    ("vec", "⃗"),
    ("vee", "∨"),
    ("veebar", "⊻"),
    ("veedot", "⟇"),
    ("veedoublebar", "⩣"),
    ("veeeq", "≚"),
    ("veemidvert", "⩛"),
    ("veeodot", "⩒"),
    ("venus", "♀"),
    ("verti", "ˌ"),
    ("vertoverlay", "⃒"),
    ("verts", "ˈ"),
    ("verymuchless", "⋘"),
    ("viewdata", "⌗"),
    ("virgo", "♍"),
    ("visiblespace", "␣"),
    ("vrectangleblack", "▮"),
    ("vrecto", "▯"),
    ("vysmblkcircle", "∙"),
    ("vysmblksquare", "⬝"),
    ("vysmwhtsquare", "⬞"),
    ("wedge", "∧"),
    ("wedgedot", "⟑"),
    ("wedgedoublebar", "⩠"),
    ("wedgemidvert", "⩚"),
    ("wedgeodot", "⩑"),
    ("wedgeonwedge", "⩕"),
    ("wedgeq", "≙"),
    ("whitearrowupfrombar", "⇪"),
    ("whiteinwhitetriangle", "⟁"),
    ("whitepointerleft", "◅"),
    ("whitepointerright", "▻"),
    ("whthorzoval", "⬭"),
    ("whtvertoval", "⬯"),
    ("wideangledown", "⦦"),
    ("wideangleup", "⦧"),
    ("widebridgeabove", "⃩"),
    ("wideutilde", "̰"),
    ("wp", "℘"),
    ("wr", "≀"),
    ("xi", "ξ"),
    ("xor", "⊻"),
    ("xrat", "℞"),
    ("yen", "¥"),
    ("yogh", "ʒ"),
    ("zeta", "ζ"),
];

// ─── FFI functions ────────────────────────────────────────────────────────────

/// Look up a Julia LaTeX symbol name (without the leading backslash).
/// Returns the Unicode string on success, or an empty string if not found.
///
/// Example: `unicode_lookup("alpha")` → `"α"`
pub fn unicode_lookup(name: String) -> String {
    match SYMBOLS.binary_search_by_key(&name.as_str(), |&(k, _)| k) {
        Ok(i) => SYMBOLS[i].1.to_string(),
        Err(_) => String::new(),
    }
}

/// Return a JSON array of all symbol names whose names start with `prefix`.
/// Each element is `{"name": "...", "char": "..."}`.
/// Returns at most 50 matches to keep the payload small.
///
/// Example: `unicode_completions_for_prefix("alp")` →
///   `[{"name":"alpha","char":"α"},{"name":"aleph","char":"ℵ"}]`
pub fn unicode_completions_for_prefix(prefix: String) -> String {
    let matches: Vec<_> = SYMBOLS
        .iter()
        .filter(|(k, _)| k.starts_with(prefix.as_str()))
        .take(50)
        .map(|(k, v)| json!({"name": k, "char": v}))
        .collect();
    json!(matches).to_string()
}

/// Map a LaTeX font command + letter to a Julia symbol table name.
/// E.g., `("mathbf", "b")` → `"bfb"`, `("mathbb", "R")` → `"bbR"`.
fn latex_font_to_julia(cmd: &str, letter: &str) -> Option<&'static str> {
    let prefix = match cmd {
        "mathbf" | "textbf" | "boldsymbol" => "bf",
        "mathbb" => "bb",
        "mathcal" | "cal" => "scr",
        "mathfrak" | "frak" => "frak",
        "mathit" | "textit" => "it",
        "mathsf" => "sf",
        "mathtt" => "tt",
        _ => return None,
    };
    // Build the Julia name (e.g., "bfb", "bbR")
    // We leak the string for a static lifetime — these are short-lived lookups
    // and the number of distinct strings is bounded.
    let julia_name = format!("{prefix}{letter}");
    SYMBOLS
        .binary_search_by_key(&julia_name.as_str(), |&(k, _)| k)
        .ok()
        .map(|i| SYMBOLS[i].1)
}

/// Scan a string for LaTeX commands and return JSON overlay data.
///
/// Input: the raw text of a math region (between $ delimiters).
/// Output: JSON array of `{"offset": N, "replacement": "X"}` objects.
///
/// Each object says: "at byte offset N from the start of this string,
/// replace the character with X". An empty replacement means "hide this char".
///
/// # Rendering pipeline
///
/// 1. **AST pass** — parse with `mathlex::parse_latex_lenient` to get semantic
///    structure (fractions, matrices, etc.).  Used to disambiguate constructs
///    that the position-only scanner cannot distinguish alone.
///
/// 2. **Overlay pass** — walk the source text byte-by-byte, emitting overlay
///    pairs.  The scanner consults the AST for semantic hints but computes all
///    positions from the source text, since mathlex does not yet provide
///    per-node source spans.
///
/// # Supported constructs
///
/// - `\name` commands (e.g., `\alpha` → `α`, `\kappa` → `κ`)
/// - `\mathbf{x}` font commands → Unicode math bold/italic/etc.
/// - `\text{...}` — keep text content, hide `\text{` and closing `}`
/// - `^{...}` superscripts (digits and +-=()ni)
/// - `_{...}` subscripts (digits and letters)
/// - `\begin{cases}...\end{cases}` → Unicode curly braces (⎧ ⎨ ⎩)
/// - `\begin{aligned}...\end{aligned}` — hide delimiters, keep content
/// - `\begin{pmatrix}...\end{pmatrix}` → parens (⎛⎜⎞ style)
/// - `\begin{bmatrix}...\end{bmatrix}` → brackets
/// - `\begin{vmatrix}...\end{vmatrix}` → bars
/// - `\begin{matrix}...\end{matrix}` — hide delimiters, keep content
/// - `\\` row separators → emit fence character
/// - `&` column separators → render as space
/// - `\|` norm delimiter → `‖`
/// Find `$...$` and `$$...$$` and `\(...\)` math regions in document text.
/// Returns (region_start, region_end) pairs for the CONTENTS (excluding delimiters).
fn find_math_regions(text: &str) -> Vec<(usize, usize)> {
    let bytes = text.as_bytes();
    let len = bytes.len();
    let mut regions = Vec::new();
    let mut i = 0;

    while i < len {
        if bytes[i] == b'$' {
            if i + 1 < len && bytes[i + 1] == b'$' {
                // $$...$$
                let start = i + 2;
                let mut j = start;
                while j + 1 < len {
                    if bytes[j] == b'$' && bytes[j + 1] == b'$' {
                        regions.push((start, j));
                        i = j + 2;
                        break;
                    }
                    j += 1;
                }
                if j + 1 >= len {
                    break;
                }
            } else {
                // $...$
                let start = i + 1;
                let mut j = start;
                while j < len {
                    if bytes[j] == b'$' {
                        regions.push((start, j));
                        i = j + 1;
                        break;
                    }
                    j += 1;
                }
                if j >= len {
                    break;
                }
            }
        } else if bytes[i] == b'\\' && i + 1 < len && bytes[i + 1] == b'(' {
            // \(...\)
            let start = i + 2;
            let mut j = start;
            while j + 1 < len {
                if bytes[j] == b'\\' && bytes[j + 1] == b')' {
                    regions.push((start, j));
                    i = j + 2;
                    break;
                }
                j += 1;
            }
            if j + 1 >= len {
                break;
            }
        } else {
            i += 1;
        }
    }

    regions
}

/// One-shot conceal computation: takes full document text, finds math regions,
/// runs the overlay scanner on each, and returns a JSON array with document-relative
/// offsets: [{"offset": N, "replacement": "X"}, ...]
pub fn compute_conceal_overlays(text: String) -> String {
    let regions = find_math_regions(&text);
    if regions.is_empty() {
        return "[]".to_string();
    }

    let mut all_overlays: Vec<serde_json::Value> = Vec::new();

    for (region_start, region_end) in regions {
        if region_end <= region_start {
            continue;
        }
        let math_text = &text[region_start..region_end];
        let json_str = latex_overlays(math_text.to_string());

        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&json_str) {
            if let Some(arr) = v.as_array() {
                for obj in arr {
                    if let Some(offset) = obj.get("offset").and_then(|o| o.as_i64()) {
                        let replacement = obj
                            .get("replacement")
                            .and_then(|o| o.as_str())
                            .unwrap_or("");
                        all_overlays.push(json!({
                            "offset": region_start + offset as usize,
                            "replacement": replacement
                        }));
                    }
                }
            }
        }
    }

    json!(all_overlays).to_string()
}

/// Like `compute_conceal_overlays`, but only scans lines that start with "# "
/// (Julia/notebook comment lines that contain markdown with LaTeX math).
/// This avoids false-positive `$` matches from Julia string interpolation.
pub fn compute_conceal_overlays_for_comments(text: String) -> String {
    let mut comment_ranges: Vec<(usize, usize)> = Vec::new();
    let mut filtered = String::new();

    for (line_start, line) in line_ranges(&text) {
        let trimmed = line.trim_end_matches('\n').trim_end_matches('\r');
        if let Some(content) = trimmed.strip_prefix("# ") {
            let content_start = line_start + (trimmed.len() - content.len());
            comment_ranges.push((content_start, filtered.len()));
            filtered.push_str(content);
            filtered.push('\n');
        }
    }

    if filtered.is_empty() {
        return "[]".to_string();
    }

    let regions = find_math_regions(&filtered);
    if regions.is_empty() {
        return "[]".to_string();
    }

    let mut all_overlays: Vec<serde_json::Value> = Vec::new();

    for (region_start, region_end) in regions {
        if region_end <= region_start {
            continue;
        }
        let math_text = &filtered[region_start..region_end];
        let json_str = latex_overlays(math_text.to_string());

        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&json_str) {
            if let Some(arr) = v.as_array() {
                for obj in arr {
                    if let Some(offset) = obj.get("offset").and_then(|o| o.as_i64()) {
                        let replacement = obj
                            .get("replacement")
                            .and_then(|o| o.as_str())
                            .unwrap_or("");
                        let filtered_pos = region_start + offset as usize;
                        let original_pos =
                            map_filtered_to_original(&comment_ranges, filtered_pos);
                        all_overlays.push(json!({
                            "offset": original_pos,
                            "replacement": replacement
                        }));
                    }
                }
            }
        }
    }

    json!(all_overlays).to_string()
}

fn line_ranges(text: &str) -> Vec<(usize, &str)> {
    let mut result = Vec::new();
    let mut start = 0;
    for line in text.split('\n') {
        result.push((start, line));
        start += line.len() + 1;
    }
    result
}

fn map_filtered_to_original(comment_ranges: &[(usize, usize)], filtered_pos: usize) -> usize {
    let mut best_orig = 0;
    let mut best_filt = 0;
    for &(orig_start, filt_start) in comment_ranges {
        if filt_start <= filtered_pos {
            best_orig = orig_start;
            best_filt = filt_start;
        } else {
            break;
        }
    }
    best_orig + (filtered_pos - best_filt)
}

pub fn latex_overlays(text: String) -> String {
    let bytes = text.as_bytes();
    let mut overlays: Vec<serde_json::Value> = Vec::new();
    let mut i = 0;

    // AST pass: parse with mathlex to get semantic structure.
    // Walk the AST to extract structural hints that improve rendering.
    // The scanner still drives positions from source bytes, but consults
    // these hints for semantic decisions (fraction depth, matrix dims, etc).
    let ast_output = mathlex::parse_latex_lenient(&text);
    let ast_hint = ast_output.expression.as_ref();
    let _max_frac_depth = ast_hint.map(fragment_frac_depth).unwrap_or(0);

    /// Track which environment we're inside and which row we're on within it.
    /// This lets us emit the right Unicode fence character on each row boundary.
    struct EnvState {
        env_name: String,
        row: usize,
        total_rows: usize,
    }

    let mut env_stack: Vec<EnvState> = Vec::new();

    // Count rows in environment content for fence character selection.
    // Row count = number of \\ delimiters + 1, within each env.
    fn count_rows_in_env(text: &str, env_start: usize, env_end: usize) -> usize {
        let content = &text[env_start..env_end];
        let mut rows = 1;
        let mut k = 0;
        let b = content.as_bytes();
        while k + 1 < b.len() {
            if b[k] == b'\\' && b[k + 1] == b'\\' {
                rows += 1;
                k += 2;
            } else {
                k += 1;
            }
        }
        rows
    }

    while i < bytes.len() {
        if bytes[i] == b'\\' && i + 1 < bytes.len() && bytes[i + 1].is_ascii_alphabetic() {
            // Parse \commandname
            let cmd_start = i;
            i += 1; // skip backslash
            let name_start = i;
            while i < bytes.len() && bytes[i].is_ascii_alphabetic() {
                i += 1;
            }
            let name = &text[name_start..i];

            // ── \begin{env_name} ──────────────────────────────────────────
            if name == "begin" {
                // Skip whitespace
                while i < bytes.len() && bytes[i] == b' ' {
                    i += 1;
                }
                if i < bytes.len() && bytes[i] == b'{' {
                    i += 1;
                    let env_name_start = i;
                    while i < bytes.len() && bytes[i] != b'}' {
                        i += 1;
                    }
                    let env_name = &text[env_name_start..i];
                    if i < bytes.len() {
                        i += 1;
                    } // skip }

                    // Find matching \end{env_name}
                    let end_tag = format!("\\end{{{}}}", env_name);
                    let env_content_start = i;
                    let env_end_pos = text[env_content_start..]
                        .find(&end_tag)
                        .map(|pos| env_content_start + pos)
                        .unwrap_or(text.len());

                    let total_rows = count_rows_in_env(&text, env_content_start, env_end_pos);

                    // Hide entire \begin{env_name} and push environment state
                    for k in cmd_start..i {
                        overlays.push(json!({"offset": k, "replacement": ""}));
                    }

                    env_stack.push(EnvState {
                        env_name: env_name.to_string(),
                        row: 0,
                        total_rows,
                    });

                    // Emit opening fence for this environment type
                    let env = env_stack.last().unwrap();
                    let fence = open_fence(&env.env_name, env.total_rows);
                    if !fence.is_empty() {
                        overlays.push(json!({"offset": cmd_start, "replacement": fence}));
                    }
                }
                continue;
            }

            // ── \end{env_name} ────────────────────────────────────────────
            if name == "end" {
                while i < bytes.len() && bytes[i] == b' ' {
                    i += 1;
                }
                if i < bytes.len() && bytes[i] == b'{' {
                    i += 1;
                    while i < bytes.len() && bytes[i] != b'}' {
                        i += 1;
                    }
                    if i < bytes.len() {
                        i += 1;
                    }

                    // Pop matching environment and emit closing fence
                    let fence = env_stack
                        .pop()
                        .map(|env| close_fence(&env.env_name, env.total_rows))
                        .unwrap_or_default();

                    // Hide entire \end{env_name}
                    for k in cmd_start..i {
                        overlays.push(json!({"offset": k, "replacement": ""}));
                    }
                    if !fence.is_empty() {
                        overlays.push(json!({"offset": i - 1, "replacement": fence}));
                    }
                }
                continue;
            }

            // ── \text{content} ────────────────────────────────────────────
            if name == "text" || name == "mathrm" || name == "operatorname" {
                while i < bytes.len() && bytes[i] == b' ' {
                    i += 1;
                }
                if i < bytes.len() && bytes[i] == b'{' {
                    // Hide \text{ or \mathrm{ or \operatorname{
                    for k in cmd_start..i + 1 {
                        overlays.push(json!({"offset": k, "replacement": ""}));
                    }
                    i += 1; // skip {
                            // Keep content as-is, just hide the closing }
                    while i < bytes.len() && bytes[i] != b'}' {
                        i += 1;
                    }
                    if i < bytes.len() {
                        overlays.push(json!({"offset": i, "replacement": ""}));
                        i += 1;
                    }
                    continue;
                }
            }

            // ── Font commands: \mathbf{x} etc ─────────────────────────────
            if matches!(
                name,
                "mathbf"
                    | "textbf"
                    | "boldsymbol"
                    | "mathbb"
                    | "mathcal"
                    | "cal"
                    | "mathfrak"
                    | "frak"
                    | "mathit"
                    | "textit"
                    | "mathsf"
                    | "mathtt"
            ) {
                let mut j = i;
                while j < bytes.len() && bytes[j] == b' ' {
                    j += 1;
                }
                if j < bytes.len() && bytes[j] == b'{' {
                    let _brace_start = j;
                    j += 1;
                    let content_start = j;
                    while j < bytes.len() && bytes[j] != b'}' {
                        j += 1;
                    }
                    if j < bytes.len() {
                        let content = &text[content_start..j];
                        j += 1;

                        if content.len() == 1 {
                            if let Some(replacement) = latex_font_to_julia(name, content) {
                                overlays
                                    .push(json!({"offset": cmd_start, "replacement": replacement}));
                                for k in (cmd_start + 1)..j {
                                    overlays.push(json!({"offset": k, "replacement": ""}));
                                }
                                i = j;
                                continue;
                            }
                        }
                        let mut any_replaced = false;
                        let mut replacements = Vec::new();
                        for ch in content.chars() {
                            if let Some(r) = latex_font_to_julia(name, &ch.to_string()) {
                                replacements.push(Some(r));
                                any_replaced = true;
                            } else {
                                replacements.push(None);
                            }
                        }
                        if any_replaced {
                            for k in cmd_start..content_start {
                                overlays.push(json!({"offset": k, "replacement": ""}));
                            }
                            let mut char_offset = content_start;
                            for (ci, ch) in content.chars().enumerate() {
                                if let Some(r) = replacements[ci] {
                                    overlays.push(json!({"offset": char_offset, "replacement": r}));
                                }
                                char_offset += ch.len_utf8();
                            }
                            overlays.push(json!({"offset": j - 1, "replacement": ""}));
                            i = j;
                            continue;
                        }
                        i = j;
                        continue;
                    }
                }
            }

            // ── \frac{num}{den} ──────────────────────────────────────────────
            if name == "frac" || name == "dfrac" || name == "tfrac" {
                let mut j = i;
                while j < bytes.len() && bytes[j] == b' ' {
                    j += 1;
                }
                if j < bytes.len() && bytes[j] == b'{' {
                    j += 1;
                    // Hide \frac{ opening
                    for k in cmd_start..j {
                        overlays.push(json!({"offset": k, "replacement": ""}));
                    }
                    // Scan numerator content until matching }
                    let mut depth = 1i32;
                    let mut num_close = j;
                    while num_close < bytes.len() && depth > 0 {
                        if bytes[num_close] == b'{' {
                            depth += 1;
                        } else if bytes[num_close] == b'}' {
                            depth -= 1;
                        }
                        num_close += 1;
                    }
                    // num_close is now past closing }
                    // Hide } closing numerator, replace with fraction slash
                    overlays.push(json!({"offset": num_close - 1, "replacement": "⁄"}));
                    // Skip whitespace before denominator
                    let mut k = num_close;
                    while k < bytes.len() && bytes[k] == b' ' {
                        k += 1;
                    }
                    if k < bytes.len() && bytes[k] == b'{' {
                        // Hide { opening denominator
                        overlays.push(json!({"offset": k, "replacement": ""}));
                        k += 1;
                        // Scan denominator content until matching }
                        depth = 1;
                        let mut den_close = k;
                        while den_close < bytes.len() && depth > 0 {
                            if bytes[den_close] == b'{' {
                                depth += 1;
                            } else if bytes[den_close] == b'}' {
                                depth -= 1;
                            }
                            den_close += 1;
                        }
                        // Hide } closing denominator
                        overlays.push(json!({"offset": den_close - 1, "replacement": ""}));
                        i = num_close;
                        continue;
                    }
                    i = num_close;
                    continue;
                }
                // Fallback: just hide the \frac command
                overlays.push(json!({"offset": cmd_start, "replacement": ""}));
                for k in (cmd_start + 1)..i {
                    overlays.push(json!({"offset": k, "replacement": ""}));
                }
                continue;
            }

            // ── Simple \name lookup ────────────────────────────────────────
            let lookup = unicode_lookup(name.to_string());
            if !lookup.is_empty() {
                overlays.push(json!({"offset": cmd_start, "replacement": lookup}));
                for k in (cmd_start + 1)..i {
                    overlays.push(json!({"offset": k, "replacement": ""}));
                }
            }
        } else if bytes[i] == b'\\'
            && i + 1 < bytes.len()
            && !bytes[i + 1].is_ascii_alphabetic()
            && bytes[i + 1] != b'\\'
        {
            // ── Non-alpha backslash sequences: \| \{ \} \( \) \[ \] \, \; \! ──
            let sym_start = i;
            let ch = bytes[i + 1];
            let replacement: Option<&str> = match ch {
                b'|' => Some("‖"),        // \| → double vertical bar
                b'{' => Some("{"),        // \{ → escaped brace (display as brace)
                b'}' => Some("}"),        // \} → escaped brace
                b'(' => None,             // \( is math region start, handled by scanner
                b')' => None,             // \) is math region end, handled by scanner
                b'[' => None,             // \[ is display math start
                b']' => None,             // \] is display math end
                b',' => Some("\u{2006}"), // \, → thin space
                b';' => Some("\u{2005}"), // \; → medium space
                b'!' => Some("\u{200B}"), // \! → negative thin space (zero-width)
                b' ' => Some(" "),        // \<space> → regular space
                _ => None,
            };
            if let Some(rep) = replacement {
                overlays.push(json!({"offset": sym_start, "replacement": rep}));
                overlays.push(json!({"offset": sym_start + 1, "replacement": ""}));
                i += 2;
            } else {
                // For \( \) \[ \] — hide the backslash, let the scanner handle the delimiter
                if ch == b'(' || ch == b')' || ch == b'[' || ch == b']' {
                    overlays.push(json!({"offset": sym_start, "replacement": ""}));
                    i += 1;
                } else {
                    i += 1;
                }
            }
        } else if bytes[i] == b'\\' && i + 1 < bytes.len() && bytes[i + 1] == b'\\' {
            // ── Row separator \\ ──────────────────────────────────────────
            if !env_stack.is_empty() {
                env_stack.last_mut().unwrap().row += 1;
                let env = env_stack.last().unwrap();
                let row_fence = mid_fence(&env.env_name, env.row, env.total_rows);
                for k in i..i + 2 {
                    overlays.push(json!({"offset": k, "replacement": ""}));
                }
                if !row_fence.is_empty() {
                    overlays.push(json!({"offset": i, "replacement": row_fence}));
                }
                i += 2;
            } else {
                i += 1;
            }
        } else if bytes[i] == b'&' && !env_stack.is_empty() {
            // ── Column separator & in math environment ─────────────────────
            overlays.push(json!({"offset": i, "replacement": " "}));
            i += 1;
        } else if bytes[i] == b'^' && i + 1 < bytes.len() && bytes[i + 1] == b'{' {
            // ── Superscript: ^{...} ───────────────────────────────────────
            let caret_pos = i;
            i += 2;
            let content_start = i;
            while i < bytes.len() && bytes[i] != b'}' {
                i += 1;
            }
            if i < bytes.len() {
                let content = &text[content_start..i];
                i += 1;

                let super_map: &[(&str, &str)] = &[
                    ("0", "⁰"),
                    ("1", "¹"),
                    ("2", "²"),
                    ("3", "³"),
                    ("4", "⁴"),
                    ("5", "⁵"),
                    ("6", "⁶"),
                    ("7", "⁷"),
                    ("8", "⁸"),
                    ("9", "⁹"),
                    ("+", "⁺"),
                    ("-", "⁻"),
                    ("=", "⁼"),
                    ("(", "⁽"),
                    (")", "⁾"),
                    ("n", "ⁿ"),
                    ("i", "ⁱ"),
                ];

                let supers: Option<Vec<&str>> = content
                    .chars()
                    .map(|c| {
                        let s = c.to_string();
                        super_map
                            .iter()
                            .find(|(k, _)| *k == s.as_str())
                            .map(|(_, v)| *v)
                    })
                    .collect();

                if let Some(supers) = supers {
                    overlays.push(json!({"offset": caret_pos, "replacement": ""}));
                    overlays.push(json!({"offset": caret_pos + 1, "replacement": ""}));
                    let mut char_offset = content_start;
                    for (ci, ch) in content.chars().enumerate() {
                        overlays.push(json!({"offset": char_offset, "replacement": supers[ci]}));
                        char_offset += ch.len_utf8();
                    }
                    overlays.push(json!({"offset": i - 1, "replacement": ""}));
                }
            }
        } else if bytes[i] == b'^'
            && i + 1 < bytes.len()
            && !bytes[i + 1].is_ascii_alphabetic()
            && bytes[i + 1] != b'{'
            && bytes[i + 1] != b'\\'
        {
            // ── Single-char superscript: ^2, ^n, etc ─────────────────────
            let ch = bytes[i + 1] as char;
            let s = ch.to_string();
            let sub_map: &[(&str, &str)] = &[
                ("0", "⁰"),
                ("1", "¹"),
                ("2", "²"),
                ("3", "³"),
                ("4", "⁴"),
                ("5", "⁵"),
                ("6", "⁶"),
                ("7", "⁷"),
                ("8", "⁸"),
                ("9", "⁹"),
                ("+", "⁺"),
                ("-", "⁻"),
                ("=", "⁼"),
                ("(", "⁽"),
                (")", "⁾"),
                ("n", "ⁿ"),
                ("i", "ⁱ"),
            ];
            if let Some((_, rep)) = sub_map.iter().find(|(k, _)| *k == s.as_str()) {
                overlays.push(json!({"offset": i, "replacement": rep.to_string()}));
                overlays.push(json!({"offset": i + 1, "replacement": ""}));
                i += 2;
            } else {
                i += 1;
            }
        } else if bytes[i] == b'_' && i + 1 < bytes.len() && bytes[i + 1] == b'{' {
            // ── Subscript: _{...} ──────────────────────────────────────────
            let underscore_pos = i;
            i += 2;
            let content_start = i;
            while i < bytes.len() && bytes[i] != b'}' {
                i += 1;
            }
            if i < bytes.len() {
                let content = &text[content_start..i];
                i += 1;

                let sub_map: &[(&str, &str)] = &[
                    ("0", "₀"),
                    ("1", "₁"),
                    ("2", "₂"),
                    ("3", "₃"),
                    ("4", "₄"),
                    ("5", "₅"),
                    ("6", "₆"),
                    ("7", "₇"),
                    ("8", "₈"),
                    ("9", "₉"),
                    ("+", "₊"),
                    ("-", "₋"),
                    ("=", "₌"),
                    ("(", "₍"),
                    (")", "₎"),
                    ("n", "ₙ"),
                    ("i", "ᵢ"),
                    ("k", "ₖ"),
                    ("j", "ⱼ"),
                    ("e", "ₑ"),
                    ("a", "ₐ"),
                    ("o", "ₒ"),
                    ("x", "ₓ"),
                    ("r", "ᵣ"),
                    ("u", "ᵤ"),
                    ("v", "ᵥ"),
                ];

                let subs: Option<Vec<&str>> = content
                    .chars()
                    .map(|c| {
                        let s = c.to_string();
                        sub_map
                            .iter()
                            .find(|(k, _)| *k == s.as_str())
                            .map(|(_, v)| *v)
                    })
                    .collect();

                if let Some(subs) = subs {
                    overlays.push(json!({"offset": underscore_pos, "replacement": ""}));
                    overlays.push(json!({"offset": underscore_pos + 1, "replacement": ""}));
                    let mut char_offset = content_start;
                    for (ci, ch) in content.chars().enumerate() {
                        overlays.push(json!({"offset": char_offset, "replacement": subs[ci]}));
                        char_offset += ch.len_utf8();
                    }
                    overlays.push(json!({"offset": i - 1, "replacement": ""}));
                }
            }
        } else if bytes[i] == b'_'
            && i + 1 < bytes.len()
            && bytes[i + 1] != b'{'
            && bytes[i + 1] != b'\\'
            && bytes[i + 1] != b'_'
        {
            // ── Single-char subscript: _n, _0, etc ─────────────────────────
            let ch = bytes[i + 1] as char;
            let s = ch.to_string();
            let sub_map: &[(&str, &str)] = &[
                ("0", "₀"),
                ("1", "₁"),
                ("2", "₂"),
                ("3", "₃"),
                ("4", "₄"),
                ("5", "₅"),
                ("6", "₆"),
                ("7", "₇"),
                ("8", "₈"),
                ("9", "₉"),
                ("+", "₊"),
                ("-", "₋"),
                ("=", "₌"),
                ("(", "₍"),
                (")", "₎"),
                ("n", "ₙ"),
                ("i", "ᵢ"),
                ("k", "ₖ"),
                ("j", "ⱼ"),
                ("e", "ₑ"),
                ("a", "ₐ"),
                ("o", "ₒ"),
                ("x", "ₓ"),
                ("r", "ᵣ"),
                ("u", "ᵤ"),
                ("v", "ᵥ"),
            ];
            if let Some((_, rep)) = sub_map.iter().find(|(k, _)| *k == s.as_str()) {
                overlays.push(json!({"offset": i, "replacement": rep.to_string()}));
                overlays.push(json!({"offset": i + 1, "replacement": ""}));
                i += 2;
            } else {
                let lookup = unicode_lookup(s.clone());
                if !lookup.is_empty() {
                    overlays.push(json!({"offset": i, "replacement": lookup}));
                    overlays.push(json!({"offset": i + 1, "replacement": ""}));
                    i += 2;
                } else {
                    i += 1;
                }
            }
        } else {
            i += 1;
        }
    }

    json!(overlays).to_string()
}

// ─── Environment fence helpers ────────────────────────────────────────────────
// These map LaTeX environments to Unicode fence characters.
// "Fence" characters are the large curly/bracket decorations that wrap
// piecewise functions, matrices, etc.

fn open_fence(env: &str, _total_rows: usize) -> &'static str {
    match env {
        "cases" => "⎧",
        "aligned" | "gathered" | "split" | "gather" | "align" => "",
        "pmatrix" => "⎛",
        "bmatrix" => "⎡",
        "vmatrix" => "│",
        "matrix" => "",
        _ => "",
    }
}

fn close_fence(env: &str, _total_rows: usize) -> &'static str {
    match env {
        "cases" => "⎩",
        "aligned" | "gathered" | "split" | "gather" | "align" => "",
        "pmatrix" => "⎞",
        "bmatrix" => "⎤",
        "vmatrix" => "│",
        "matrix" => "",
        _ => "",
    }
}

fn mid_fence(env: &str, _row: usize, _total_rows: usize) -> &'static str {
    match env {
        "cases" => "⎨",
        "pmatrix" => "⎜",
        "bmatrix" => "⎢",
        "aligned" | "gathered" | "split" | "gather" | "align" | "matrix" => "",
        "vmatrix" => "│",
        _ => "",
    }
}

fn fragment_frac_depth(expr: &Expression) -> usize {
    match expr {
        Expression::Binary {
            op: mathlex::ast::BinaryOp::Div,
            left,
            right,
        } => 1 + fragment_frac_depth(left).max(fragment_frac_depth(right)),
        Expression::Binary { left, right, .. } => {
            fragment_frac_depth(left).max(fragment_frac_depth(right))
        }
        Expression::Unary { operand, .. } => fragment_frac_depth(operand),
        Expression::Function { args, .. } => {
            args.iter().map(fragment_frac_depth).max().unwrap_or(0)
        }
        Expression::Matrix(rows) => rows
            .iter()
            .flat_map(|r| r.iter())
            .map(fragment_frac_depth)
            .max()
            .unwrap_or(0),
        Expression::Vector(elems) => elems.iter().map(fragment_frac_depth).max().unwrap_or(0),
        Expression::Equation { left, right } => {
            fragment_frac_depth(left).max(fragment_frac_depth(right))
        }
        Expression::Inequality { left, right, .. } => {
            fragment_frac_depth(left).max(fragment_frac_depth(right))
        }
        _ => 0,
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_alpha() {
        assert_eq!(unicode_lookup("alpha".into()), "α");
    }

    #[test]
    fn lookup_in() {
        assert_eq!(unicode_lookup("in".into()), "∈");
    }

    #[test]
    fn lookup_pi() {
        assert_eq!(unicode_lookup("pi".into()), "π");
    }

    #[test]
    fn lookup_missing() {
        assert_eq!(unicode_lookup("notareal symbol".into()), "");
    }

    #[test]
    fn completions_prefix() {
        let result = unicode_completions_for_prefix("alp".into());
        let v: serde_json::Value = serde_json::from_str(&result).unwrap();
        let arr = v.as_array().unwrap();
        assert!(!arr.is_empty());
        assert!(arr.iter().any(|e| e["name"] == "alpha"));
    }

    #[test]
    fn completions_empty_prefix_capped() {
        let result = unicode_completions_for_prefix("".into());
        let v: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(v.as_array().unwrap().len() <= 50);
    }

    #[test]
    fn latex_overlays_simple_command() {
        let result = latex_overlays(r"\alpha + \beta".into());
        let v: serde_json::Value = serde_json::from_str(&result).unwrap();
        let arr = v.as_array().unwrap();
        // \alpha is 6 chars: overlay at 0 with α, hide 1-5
        assert!(!arr.is_empty());
        assert_eq!(arr[0]["offset"], 0);
        assert_eq!(arr[0]["replacement"], "α");
        assert_eq!(arr[1]["replacement"], "");
    }

    #[test]
    fn latex_overlays_superscript() {
        let result = latex_overlays("10^{-6}".into());
        let v: serde_json::Value = serde_json::from_str(&result).unwrap();
        let arr = v.as_array().unwrap();
        // Should produce overlays for ^, {, -, 6, }
        assert!(!arr.is_empty());
        // Check that - becomes ⁻ and 6 becomes ⁶
        let replacements: Vec<&str> = arr
            .iter()
            .map(|o| o["replacement"].as_str().unwrap())
            .collect();
        assert!(replacements.contains(&"⁻"));
        assert!(replacements.contains(&"⁶"));
    }

    #[test]
    fn latex_overlays_mathbf() {
        let result = latex_overlays(r"\mathbf{b}".into());
        let v: serde_json::Value = serde_json::from_str(&result).unwrap();
        let arr = v.as_array().unwrap();
        assert!(!arr.is_empty());
        // First overlay should replace with 𝐛
        assert_eq!(arr[0]["replacement"], "𝐛");
    }

    #[test]
    fn latex_overlays_empty() {
        let result = latex_overlays("x + y = z".into());
        let v: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(v.as_array().unwrap().is_empty());
    }

    #[test]
    fn table_is_sorted() {
        for i in 1..SYMBOLS.len() {
            assert!(
                SYMBOLS[i - 1].0 < SYMBOLS[i].0,
                "Table not sorted at index {i}: {:?} >= {:?}",
                SYMBOLS[i - 1].0,
                SYMBOLS[i].0
            );
        }
    }

    #[test]
    fn latex_overlays_text_command() {
        let result = latex_overlays(r"\text{otherwise}".into());
        let v: serde_json::Value = serde_json::from_str(&result).unwrap();
        let arr = v.as_array().unwrap();
        // \text{ should be hidden, } should be hidden, content preserved
        let hidden: Vec<(i64, &str)> = arr
            .iter()
            .filter_map(|o| {
                let off = o["offset"].as_i64().unwrap();
                let rep = o["replacement"].as_str().unwrap();
                if rep.is_empty() {
                    Some((off, rep))
                } else {
                    None
                }
            })
            .collect();
        // The backslash, "text", and "{" should be hidden
        assert!(hidden.iter().any(|(off, _)| *off == 0));
        // The closing } should be hidden
        assert!(hidden
            .iter()
            .any(|(off, _)| *off as usize == r"\text{otherwise}".len() - 1));
    }

    #[test]
    fn latex_overlays_cases_env() {
        let input = r"\begin{cases} 1 & 0 \leq n \leq 2 \\ 0 & \text{otherwise} \end{cases}";
        let result = latex_overlays(input.to_string());
        let v: serde_json::Value = serde_json::from_str(&result).unwrap();
        let arr = v.as_array().unwrap();
        let replacements: Vec<&str> = arr
            .iter()
            .map(|o| o["replacement"].as_str().unwrap())
            .collect();
        // Should have ⎧ (open fence) and ⎩ (close fence) and ⎨ (mid fence)
        assert!(
            replacements.contains(&"⎧"),
            "Expected ⎧ in {:?}",
            replacements
        );
        assert!(
            replacements.contains(&"⎩"),
            "Expected ⎩ in {:?}",
            replacements
        );
        assert!(
            replacements.contains(&"⎨"),
            "Expected ⎨ in {:?}",
            replacements
        );
        // Should have ≤ from \leq
        assert!(
            replacements.contains(&"≤"),
            "Expected ≤ from \\leq in {:?}",
            replacements
        );
        // & should become space
        assert!(
            replacements.contains(&" "),
            "Expected space from & in {:?}",
            replacements
        );
    }

    #[test]
    fn latex_overlays_pmatrix_env() {
        let input = r"\begin{pmatrix} 1 & 0 \\ 0 & 1 \end{pmatrix}";
        let result = latex_overlays(input.to_string());
        let v: serde_json::Value = serde_json::from_str(&result).unwrap();
        let arr = v.as_array().unwrap();
        let replacements: Vec<&str> = arr
            .iter()
            .map(|o| o["replacement"].as_str().unwrap())
            .collect();
        assert!(replacements.contains(&"⎛"));
        assert!(replacements.contains(&"⎜"));
        assert!(replacements.contains(&"⎞"));
    }

    #[test]
    fn latex_overlays_subscript() {
        let result = latex_overlays("h_{n}".into());
        let v: serde_json::Value = serde_json::from_str(&result).unwrap();
        let arr = v.as_array().unwrap();
        let replacements: Vec<&str> = arr
            .iter()
            .map(|o| o["replacement"].as_str().unwrap())
            .collect();
        assert!(
            replacements.contains(&"ₙ"),
            "Expected ₙ subscript in {:?}",
            replacements
        );
    }

    #[test]
    fn latex_overlays_norm_delimiter() {
        let result = latex_overlays(r"\|B - B_1\|".into());
        let v: serde_json::Value = serde_json::from_str(&result).unwrap();
        let arr = v.as_array().unwrap();
        let replacements: Vec<&str> = arr
            .iter()
            .map(|o| o["replacement"].as_str().unwrap())
            .collect();
        assert!(
            replacements.contains(&"‖"),
            "Expected ‖ from \\| in {:?}",
            replacements
        );
    }

    #[test]
    fn latex_overlays_user_example() {
        let input = r"h_n = \begin{cases} 1 & 0 \leq n \leq 2 \\ 0 & \text{otherwise} \end{cases}";
        let result = latex_overlays(input.to_string());
        let v: serde_json::Value = serde_json::from_str(&result).unwrap();
        let arr = v.as_array().unwrap();
        let replacements: Vec<&str> = arr
            .iter()
            .map(|o| o["replacement"].as_str().unwrap())
            .collect();
        assert!(
            replacements.contains(&"ₙ"),
            "Expected ₙ in {:?}",
            replacements
        );
        assert!(
            replacements.contains(&"⎧"),
            "Expected ⎧ in {:?}",
            replacements
        );
        assert!(
            replacements.contains(&"⎨"),
            "Expected ⎨ in {:?}",
            replacements
        );
        assert!(
            replacements.contains(&"⎩"),
            "Expected ⎩ in {:?}",
            replacements
        );
        assert!(
            replacements.contains(&"≤"),
            "Expected ≤ in {:?}",
            replacements
        );
    }

    #[test]
    fn latex_overlays_frac() {
        let result = latex_overlays(r"\frac{a}{b}".into());
        let v: serde_json::Value = serde_json::from_str(&result).unwrap();
        let arr = v.as_array().unwrap();
        let replacements: Vec<&str> = arr
            .iter()
            .map(|o| o["replacement"].as_str().unwrap())
            .collect();
        assert!(
            replacements.contains(&"⁄"),
            "Expected ⁄ fraction slash in {:?}",
            replacements
        );
        let hidden_offsets: Vec<i64> = arr
            .iter()
            .filter(|o| o["replacement"].as_str().unwrap() == "")
            .map(|o| o["offset"].as_i64().unwrap())
            .collect();
        assert!(
            hidden_offsets.contains(&0),
            "\\frac at offset 0 should be hidden"
        );
    }

    #[test]
    fn latex_overlays_frac_nested() {
        let result = latex_overlays(r"\frac{1}{\frac{a}{b}}".into());
        let v: serde_json::Value = serde_json::from_str(&result).unwrap();
        let arr = v.as_array().unwrap();
        let replacements: Vec<&str> = arr
            .iter()
            .map(|o| o["replacement"].as_str().unwrap())
            .collect();
        let frac_count = replacements.iter().filter(|&&r| r == "⁄").count();
        assert!(
            frac_count >= 2,
            "Expected at least 2 fraction slashes in nested frac, got {}",
            frac_count
        );
    }

    #[test]
    fn compute_conceal_overlays_with_math_region() {
        let input = r#"some text $\alpha + \beta$ more text"#;
        let result = compute_conceal_overlays(input.to_string());
        let v: serde_json::Value = serde_json::from_str(&result).unwrap();
        let arr = v.as_array().unwrap();
        assert!(!arr.is_empty(), "Expected overlays for math region");
        let offsets: Vec<usize> = arr
            .iter()
            .map(|o| o["offset"].as_u64().unwrap() as usize)
            .collect();
        let alpha_offset = "some text $".len();
        assert!(
            offsets.contains(&alpha_offset),
            "Expected alpha at offset {} in: {:?}",
            alpha_offset,
            offsets
        );
    }

    #[test]
    fn compute_conceal_overlays_no_math() {
        let result = compute_conceal_overlays("plain text no math".to_string());
        assert_eq!(result, "[]");
    }
}
