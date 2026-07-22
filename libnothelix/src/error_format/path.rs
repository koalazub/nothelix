const NIX_STORE_HASH_LEN: usize = 32;

pub(super) fn clean(path: &str) -> String {
    if path == "none" || path == "<cell>" || path.starts_with("REPL") {
        return path.to_string();
    }
    if path.contains("/nix/store/") {
        if let Some(idx) = path.find("/stdlib/") {
            let after = &path[idx + "/stdlib/".len()..];
            let cleaned = match after.find('/') {
                Some(slash) => &after[slash + 1..],
                None => after,
            };
            return format!("stdlib:{cleaned}");
        }
        if let Some(idx) = path.find("/share/julia/") {
            return path[idx + "/share/julia/".len()..].to_string();
        }
        if let Some(rest) = path.strip_prefix("/nix/store/")
            && rest.len() > NIX_STORE_HASH_LEN + 1
            && rest.as_bytes()[NIX_STORE_HASH_LEN] == b'-'
        {
            return rest[NIX_STORE_HASH_LEN + 1..].to_string();
        }
    }
    if let Some(idx) = path.find("/.julia/packages/") {
        let after = &path[idx + "/.julia/packages/".len()..];
        let parts: Vec<&str> = after.splitn(3, '/').collect();
        if let [package, _version, rest] = parts.as_slice() {
            return format!("{package}/{rest}");
        }
    }
    let components: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    if components.len() > 3 {
        return format!("…/{}", components[components.len() - 3..].join("/"));
    }
    path.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_nix_stdlib_path() {
        let p = "/nix/store/8h9qwxffgyisf9hiscw5ms6l56w6mni5-julia-bin-1.12.5/share/julia/stdlib/v1.12/LinearAlgebra/src/generic.jl";
        assert_eq!(clean(p), "stdlib:LinearAlgebra/src/generic.jl");
    }

    #[test]
    fn clean_julia_packages_path() {
        let p = "/home/user/.julia/packages/DataFrames/AbCdE/src/dataframe.jl";
        assert_eq!(clean(p), "DataFrames/src/dataframe.jl");
    }
}
