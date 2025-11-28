module ASTAnalysis

export extract_defines, extract_uses, analyze_code

# Built-in names to ignore
const BUILTINS = Set{Symbol}([
    # Keywords
    :begin, :end, :if, :else, :elseif, :for, :while, :try, :catch, :finally,
    :return, :break, :continue, :function, :macro, :module, :struct, :mutable,
    :import, :using, :export, :const, :local, :global, :let, :do, :quote,
    :nothing, :missing,
    # Types
    :Int, :Float64, :String, :Bool, :Any, :Nothing, :Vector, :Matrix, :Array,
    :Dict, :Set, :Tuple, :NamedTuple, :Pair,
    # Common functions
    :println, :print, :show, :display, :repr, :string, :typeof, :sizeof,
    :length, :size, :push!, :pop!, :append!, :insert!, :delete!, :empty!,
    :map, :filter, :reduce, :foldl, :foldr, :zip, :enumerate,
    :first, :last, :collect, :sort, :sort!, :reverse, :reverse!,
    :sum, :prod, :mean, :std, :var, :min, :max, :minimum, :maximum,
    :sqrt, :exp, :log, :sin, :cos, :tan, :abs, :floor, :ceil, :round,
    :rand, :randn, :zeros, :ones, :fill, :range, :LinRange,
    :open, :close, :read, :write, :readline, :readlines,
    :error, :throw, :rethrow,
    # Modules
    :Main, :Base, :Core,
])

# Extract variables defined by an expression
function extract_defines(expr)::Set{Symbol}
    defines = Set{Symbol}()
    _extract_defines!(defines, expr)
    defines
end

function _extract_defines!(defines::Set{Symbol}, expr)
    expr === nothing && return

    if expr isa Symbol
        return
    elseif expr isa Expr
        head = expr.head

        if head == :(=)
            # Assignment: x = ...
            lhs = expr.args[1]
            if lhs isa Symbol
                push!(defines, lhs)
            elseif lhs isa Expr && lhs.head == :tuple
                # Tuple unpacking: (a, b) = ...
                for arg in lhs.args
                    arg isa Symbol && push!(defines, arg)
                end
            elseif lhs isa Expr && lhs.head == :ref
                # Array assignment: arr[i] = ... - doesn't define arr
            end
            # Also check RHS for nested defines
            _extract_defines!(defines, expr.args[2])

        elseif head == :function || head == :(->)
            # Function definition
            if length(expr.args) >= 1
                sig = expr.args[1]
                if sig isa Expr && sig.head == :call && length(sig.args) >= 1
                    fname = sig.args[1]
                    fname isa Symbol && push!(defines, fname)
                elseif sig isa Symbol
                    push!(defines, sig)
                end
            end
            # Don't recurse into function body for top-level defines

        elseif head == :macro
            # Macro definition
            if length(expr.args) >= 1
                sig = expr.args[1]
                if sig isa Expr && sig.head == :call && length(sig.args) >= 1
                    mname = sig.args[1]
                    mname isa Symbol && push!(defines, mname)
                end
            end

        elseif head == :struct || head == :mutable
            # Struct definition
            if length(expr.args) >= 1
                sname = expr.args[1]
                if sname isa Symbol
                    push!(defines, sname)
                elseif sname isa Expr && sname.head == :(<:)
                    sname.args[1] isa Symbol && push!(defines, sname.args[1])
                end
            end

        elseif head == :const
            # const x = ...
            for arg in expr.args
                _extract_defines!(defines, arg)
            end

        elseif head in (:local, :global)
            for arg in expr.args
                if arg isa Symbol
                    push!(defines, arg)
                elseif arg isa Expr
                    _extract_defines!(defines, arg)
                end
            end

        elseif head == :for
            # for i in ... defines i
            if length(expr.args) >= 1
                iter_expr = expr.args[1]
                if iter_expr isa Expr && iter_expr.head == :(=)
                    lhs = iter_expr.args[1]
                    lhs isa Symbol && push!(defines, lhs)
                end
            end
            # Recurse into body
            for i in 2:length(expr.args)
                _extract_defines!(defines, expr.args[i])
            end

        elseif head in (:block, :let, :if, :while, :try)
            for arg in expr.args
                _extract_defines!(defines, arg)
            end
        end
    end
end

# Extract variables used by an expression
function extract_uses(expr)::Set{Symbol}
    uses = Set{Symbol}()
    local_defines = Set{Symbol}()
    _extract_uses!(uses, local_defines, expr)
    setdiff!(uses, local_defines)
    setdiff!(uses, BUILTINS)
    uses
end

function _extract_uses!(uses::Set{Symbol}, local_defines::Set{Symbol}, expr)
    expr === nothing && return

    if expr isa Symbol
        if !(expr in BUILTINS) && !(expr in local_defines)
            push!(uses, expr)
        end
    elseif expr isa Expr
        head = expr.head

        if head == :(=)
            lhs = expr.args[1]
            # Track local define
            if lhs isa Symbol
                push!(local_defines, lhs)
            end
            # RHS uses
            length(expr.args) >= 2 && _extract_uses!(uses, local_defines, expr.args[2])

        elseif head == :call
            # Function call: first arg is function name
            fname = expr.args[1]
            if fname isa Symbol && !(fname in BUILTINS)
                push!(uses, fname)
            elseif fname isa Expr
                _extract_uses!(uses, local_defines, fname)
            end
            # Arguments
            for i in 2:length(expr.args)
                _extract_uses!(uses, local_defines, expr.args[i])
            end

        elseif head == :(.)
            # Field access: x.y - x is used, y is a field name
            length(expr.args) >= 1 && _extract_uses!(uses, local_defines, expr.args[1])

        elseif head == :ref
            # Array indexing: arr[i] - both arr and i are used
            for arg in expr.args
                _extract_uses!(uses, local_defines, arg)
            end

        elseif head == :function || head == :(->)
            # Don't recurse into function definitions for uses
            # (they have their own scope)
            return

        elseif head == :for
            # Loop variable is local
            if length(expr.args) >= 1
                iter_expr = expr.args[1]
                if iter_expr isa Expr && iter_expr.head == :(=)
                    lhs = iter_expr.args[1]
                    lhs isa Symbol && push!(local_defines, lhs)
                    # The iterator expression IS used
                    length(iter_expr.args) >= 2 && _extract_uses!(uses, local_defines, iter_expr.args[2])
                end
            end
            # Body
            for i in 2:length(expr.args)
                _extract_uses!(uses, local_defines, expr.args[i])
            end

        elseif head == :macrocall
            # Macro call: @foo(args...)
            # Skip macro name (first arg), process rest
            for i in 3:length(expr.args)  # Skip macro name and line number
                _extract_uses!(uses, local_defines, expr.args[i])
            end

        else
            # Default: recurse into all args
            for arg in expr.args
                _extract_uses!(uses, local_defines, arg)
            end
        end
    end
end

# High-level function to analyze code string
function analyze_code(code::String)
    expr = Meta.parse("begin\n$code\nend")
    defines = extract_defines(expr)
    uses = extract_uses(expr)
    (defines=defines, uses=uses)
end

end # module
