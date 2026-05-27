pub(crate) fn sanitize_path_part(value: &str) -> String {
    value
        .chars()
        .map(|ch| match ch {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' | '.' | '=' => ch,
            _ => '_',
        })
        .collect()
}
