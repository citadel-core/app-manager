use lazy_static::lazy_static;
use regex::Regex;

use crate::composegenerator::types::Permissions;

lazy_static! {
    // This should have been the following regex originally: \$(\{.*?}|[A-z1-9]+)
    // However, it lead to a double match of ${VAR} and {VAR} getting matched for some reason
    static ref ENV_VAR_REGEX: Regex = Regex::new(r"\$\{.*?}|\$[A-z1-9]+").unwrap();
}

#[macro_export]
macro_rules! map(
    { $($key:expr => $value:expr),+ } => {
        {
            let mut m = ::std::collections::HashMap::new();
            $(
                m.insert($key.to_string(), $value);
            )+
            m
        }
     };
);

#[macro_export]
macro_rules! bmap(
    { $($key:expr => $value:expr),+ } => {
        {
            let mut m = ::std::collections::BTreeMap::new();
            $(
                m.insert($key.to_string(), $value);
            )+
            m
        }
     };
);

pub fn find_env_vars(string: &str) -> Vec<&str> {
    let mut result: Vec<&str> = Vec::new();
    let matches = ENV_VAR_REGEX.captures_iter(string);
    for captures in matches {
        for element in captures.iter().flatten() {
            let matched = element.as_str();
            // If the env var starts with ${, remove it and the closing }
            // Otherwise, just remove the $
            if matched.starts_with("${") {
                let simplified = &matched[2..matched.len() - 1];
                // Split it at :-, : or -, depending on which of these exist
                let split = simplified.splitn(2, '-').collect::<Vec<&str>>();
                let main_var = split[0].split(':').collect::<Vec<&str>>()[0];
                result.push(main_var);
                if split.len() > 1 {
                    let mut env_vars_in_default = find_env_vars(split[1]);
                    result.append(&mut env_vars_in_default);
                }
            } else {
                result.push(&matched[1..matched.len()]);
            };
        }
    }
    result
}

#[cfg(test)]
mod test_env_vars {
    use crate::utils::find_env_vars;
    use pretty_assertions::assert_eq;

    #[test]
    fn handle_empty_properly() {
        let result = find_env_vars("Example value 123$ test");
        assert_eq!(result, Vec::<String>::new());
    }

    #[test]
    fn find_syntax_1() {
        let result = find_env_vars("something${BITCOIN_IP}something");
        assert_eq!(result, vec!["BITCOIN_IP"]);
    }

    #[test]
    fn find_syntax_2() {
        let result = find_env_vars("something $BITCOIN_IP something");
        assert_eq!(result, vec!["BITCOIN_IP"]);
    }

    #[test]
    fn find_syntax_combined() {
        let result = find_env_vars("something $BITCOIN_IP something ${LND_IP} $ANOTHER_THING");
        let expected = vec!["BITCOIN_IP", "LND_IP", "ANOTHER_THING"];

        assert!(expected.iter().all(|item| result.contains(item)));
    }

    #[test]
    fn find_vars_with_fallback() {
        let result = find_env_vars("${DEVICE_HOSTS:-\"Hello world\"} ${OTHER_VAR-\"Hello world\"} ${ANOTHER_VAR:2:1} ${LAST_VAR:-$BYPASS}");
        assert_eq!(
            result,
            vec![
                "DEVICE_HOSTS",
                "OTHER_VAR",
                "ANOTHER_VAR",
                "LAST_VAR",
                "BYPASS"
            ]
        );
    }
}

pub fn flatten<'a>(perms: &'a Vec<Permissions>) -> Vec<&'a String> {
    let mut result = Vec::<&'a String>::new();
    for perm in perms {
        match perm {
            Permissions::OneDependency(dependency) => {
                result.push(dependency);
            }
            Permissions::AlternativeDependency(dependencies) => {
                for dependency in dependencies {
                    result.push(dependency);
                }
            }
        }
    }
    result
}

#[cfg(test)]
mod test_flatten {
    use crate::composegenerator::types::Permissions;
    use crate::utils::flatten;

    #[test]
    fn handle_empty_properly() {
        let perms = Vec::<Permissions>::new();
        let result = flatten(&perms);
        assert_eq!(result, Vec::<&String>::new());
    }

    #[test]
    fn handle_simple_properly() {
        let perms = vec![
            Permissions::OneDependency("a".to_string()),
            Permissions::OneDependency("b".to_string()),
        ];
        let result = flatten(&perms);
        assert_eq!(result, vec![&"a".to_string(), &"b".to_string()]);
    }

    #[test]
    fn handle_alternating_properly() {
        let perms = vec![
            Permissions::OneDependency("a".to_string()),
            Permissions::AlternativeDependency(vec!["b".to_string(), "c".to_string()]),
        ];
        let result = flatten(&perms);
        assert_eq!(
            result,
            vec![&"a".to_string(), &"b".to_string(), &"c".to_string()]
        );
    }
}
