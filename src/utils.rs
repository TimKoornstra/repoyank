/// Rough estimate: GPT-style token ≈ 4 chars (good enough for UI)
pub fn approx_tokens(s: &str) -> usize {
    s.chars().count() / 4
}
