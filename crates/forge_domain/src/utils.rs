use regex::Regex;

pub fn sanitize_for_json(input: &str) -> String {
    let re = Regex::new(r"[\x00-\x1F]").unwrap();
    re.replace_all(input, "").to_string()
}