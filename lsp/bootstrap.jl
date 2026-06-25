# SymbolServer master breaks on Julia 1.14-DEV; symbolserver-1.14.patch fixes
# it at the source (new TypeEq type-lattice node, module-self-ref cycle, module
# alias resolution). Depot packages are read-only and Pkg may re-extract them,
# so the patch goes on a writable Pkg.develop copy. Pinned to a master commit
# so the patch context is stable.
#
# ARGS: 1=LSP dir  2=patch file  3=NothelixMacros path ("" to skip)

using Pkg

ENV["JULIA_PKG_PRECOMPILE_AUTO"] = "0"

const LSPDIR = ARGS[1]
const PATCHFILE = ARGS[2]
const MACROS = length(ARGS) >= 3 ? ARGS[3] : ""
const SSID = Base.PkgId(Base.UUID("cf896787-08d5-524d-9de7-132aaa0cb996"), "SymbolServer")
const SS_URL = "https://github.com/julia-vscode/SymbolServer.jl"
const SS_REV = "9678f3dfa76e7c2b7216a20d39f834cab7057829"
const PATCH_MARKER = "val === vr ? nothing"

dev = joinpath(LSPDIR, "dev", "SymbolServer")
dev_utils = joinpath(dev, "src", "utils.jl")
patched = isfile(dev_utils) && occursin(PATCH_MARKER, read(dev_utils, String))

if !patched
    Pkg.add(Pkg.PackageSpec(url=SS_URL, rev=SS_REV))
    src = dirname(dirname(Base.locate_package(SSID)))
    rm(dev; recursive=true, force=true)
    mkpath(dirname(dev))
    cp(src, dev)
    chmod(dev, 0o755)
    for (root, dirs, files) in walkdir(dev)
        for d in dirs
            chmod(joinpath(root, d), 0o755)
        end
        for f in files
            chmod(joinpath(root, f), 0o644)
        end
    end
    run(pipeline(ignorestatus(`patch -N -p1 -d $dev`); stdin=PATCHFILE))
    occursin(PATCH_MARKER, read(dev_utils, String)) || error("SymbolServer patch did not apply")
    for (root, _, files) in walkdir(dev), f in files
        endswith(f, ".rej") && rm(joinpath(root, f); force=true)
    end
    Pkg.develop(Pkg.PackageSpec(path=dev))
end

Pkg.instantiate()

if !isempty(MACROS) && isdir(MACROS)
    try
        Pkg.develop(Pkg.PackageSpec(path=MACROS))
    catch
    end
end

Pkg.precompile()

using LanguageServer
println("OK")
