"""
    NothelixMacros

Tiny stub package that exports the `@cell` and `@markdown` macros
used as cell markers in nothelix notebook files (`.jl`). Both macros
expand to `nothing` — they exist purely so that:

1. Julia's parser accepts the marker lines without error.
2. LanguageServer.jl's StaticLint resolves them as real exported
   symbols instead of flagging "Missing reference: @cell" on every
   cell header.

The julia-lsp wrapper `dev`s this package into the analysed env at
startup so the macros are available without the user adding
`using NothelixMacros` to their notebook files.
"""
module NothelixMacros

export @cell, @markdown

"""
    @cell N :lang ["label"]

Mark the start of a code cell. Expands to `nothing` — the marker is
consumed by the nothelix plugin, not by Julia.
"""
macro cell(args...)
    nothing
end

"""
    @markdown N ["label"]

Mark the start of a markdown cell. Expands to `nothing`.
"""
macro markdown(args...)
    nothing
end

end # module
