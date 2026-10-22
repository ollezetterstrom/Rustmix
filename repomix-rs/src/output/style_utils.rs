//! Helpers shared by the output formatters.

/// Map a file extension to the language name used in markdown code fences.
const EXTENSION_TO_LANGUAGE: &[(&str, &str)] = &[
    ("js", "javascript"),
    ("mjs", "javascript"),
    ("cjs", "javascript"),
    ("jsx", "javascript"),
    ("ts", "typescript"),
    ("mts", "typescript"),
    ("cts", "typescript"),
    ("tsx", "typescript"),
    ("vue", "vue"),
    ("svelte", "svelte"),
    ("astro", "astro"),
    ("py", "python"),
    ("pyw", "python"),
    ("pyi", "python"),
    ("rb", "ruby"),
    ("erb", "erb"),
    ("java", "java"),
    ("kt", "kotlin"),
    ("kts", "kotlin"),
    ("scala", "scala"),
    ("groovy", "groovy"),
    ("clj", "clojure"),
    ("cljs", "clojure"),
    ("cljc", "clojure"),
    ("c", "c"),
    ("h", "c"),
    ("cpp", "cpp"),
    ("cc", "cpp"),
    ("cxx", "cpp"),
    ("hpp", "cpp"),
    ("hxx", "cpp"),
    ("m", "objectivec"),
    ("mm", "objectivec"),
    ("cs", "csharp"),
    ("fs", "fsharp"),
    ("fsx", "fsharp"),
    ("fsi", "fsharp"),
    ("vb", "vb"),
    ("cshtml", "razor"),
    ("razor", "razor"),
    ("go", "go"),
    ("rs", "rust"),
    ("swift", "swift"),
    ("php", "php"),
    ("dart", "dart"),
    ("haml", "haml"),
    ("slim", "slim"),
    ("hs", "haskell"),
    ("lhs", "haskell"),
    ("ex", "elixir"),
    ("exs", "elixir"),
    ("erl", "erlang"),
    ("hrl", "erlang"),
    ("ml", "ocaml"),
    ("mli", "ocaml"),
    ("elm", "elm"),
    ("r", "r"),
    ("jl", "julia"),
    ("nim", "nim"),
    ("zig", "zig"),
    ("v", "v"),
    ("lua", "lua"),
    ("pl", "perl"),
    ("pm", "perl"),
    ("raku", "raku"),
    ("sh", "bash"),
    ("bash", "bash"),
    ("zsh", "zsh"),
    ("fish", "fish"),
    ("ps1", "powershell"),
    ("psm1", "powershell"),
    ("bat", "batch"),
    ("cmd", "batch"),
    ("awk", "awk"),
    ("html", "html"),
    ("htm", "html"),
    ("xhtml", "html"),
    ("css", "css"),
    ("scss", "scss"),
    ("sass", "sass"),
    ("less", "less"),
    ("styl", "stylus"),
    ("json", "json"),
    ("json5", "json5"),
    ("jsonc", "json"),
    ("xml", "xml"),
    ("xsl", "xml"),
    ("xslt", "xml"),
    ("svg", "xml"),
    ("yaml", "yaml"),
    ("yml", "yaml"),
    ("toml", "toml"),
    ("ini", "ini"),
    ("cfg", "ini"),
    ("conf", "ini"),
    ("md", "markdown"),
    ("mdx", "markdown"),
    ("rst", "rst"),
    ("tex", "latex"),
    ("latex", "latex"),
    ("sql", "sql"),
    ("prisma", "prisma"),
    ("dockerfile", "dockerfile"),
    ("tf", "hcl"),
    ("tfvars", "hcl"),
    ("hcl", "hcl"),
    ("nix", "nix"),
    ("nginx", "nginx"),
    ("apacheconf", "apacheconf"),
    ("cmake", "cmake"),
    ("makefile", "makefile"),
    ("mk", "makefile"),
    ("glsl", "glsl"),
    ("vert", "glsl"),
    ("frag", "glsl"),
    ("wgsl", "wgsl"),
    ("hlsl", "hlsl"),
    ("vhdl", "vhdl"),
    ("vhd", "vhdl"),
    ("sol", "solidity"),
    ("asm", "asm"),
    ("s", "asm"),
    ("hbs", "handlebars"),
    ("handlebars", "handlebars"),
    ("mustache", "handlebars"),
    ("ejs", "ejs"),
    ("jinja", "jinja"),
    ("jinja2", "jinja"),
    ("j2", "jinja"),
    ("liquid", "liquid"),
    ("njk", "nunjucks"),
    ("pug", "pug"),
    ("jade", "pug"),
    ("twig", "twig"),
    ("graphql", "graphql"),
    ("gql", "graphql"),
    ("proto", "protobuf"),
    ("coffee", "coffeescript"),
    ("vim", "vim"),
    ("diff", "diff"),
    ("patch", "diff"),
    ("wasm", "wasm"),
    ("wat", "wasm"),
];

/// The language name for a file path (used for markdown code fences).
pub fn language_from_file_path(file_path: &str) -> &'static str {
    let basename = file_path
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(file_path);
    let Some(ext) = basename.rsplit('.').next() else {
        return "";
    };
    if basename == ext {
        // No dot in the name.
        return "";
    }
    let ext = ext.to_ascii_lowercase();
    EXTENSION_TO_LANGUAGE
        .iter()
        .find(|(k, _)| *k == ext)
        .map(|(_, v)| *v)
        .unwrap_or("")
}

/// Escape text for inclusion in XML.
///
/// The five characters that have to be escaped are all ASCII, and an ASCII
/// character never appears inside a multi byte UTF-8 sequence - so the text
/// can be copied in blocks between the special characters.
pub fn escape_xml(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len() + text.len() / 16);
    let mut last = 0usize;
    for (index, &byte) in bytes.iter().enumerate() {
        let escaped = match byte {
            b'&' => "&amp;",
            b'<' => "&lt;",
            b'>' => "&gt;",
            b'"' => "&quot;",
            b'\'' => "&apos;",
            _ => continue,
        };
        if last < index {
            out.push_str(&text[last..index]);
        }
        out.push_str(escaped);
        last = index + 1;
    }
    if last < text.len() {
        out.push_str(&text[last..]);
    }
    out
}

/// Is this a character that JavaScript's `String.prototype.trim` removes?
///
/// This is the exact set from the ECMAScript specification: the `WhiteSpace`
/// and `LineTerminator` productions. It is *not* the same as Rust's
/// `char::is_whitespace`, which also trims `U+0085` (NEL).
pub fn is_js_whitespace(c: char) -> bool {
    matches!(c,
        '\u{0009}'..='\u{000D}'
        | '\u{0020}'
        | '\u{00A0}'
        | '\u{1680}'
        | '\u{2000}'..='\u{200A}'
        | '\u{2028}'
        | '\u{2029}'
        | '\u{202F}'
        | '\u{205F}'
        | '\u{3000}'
        | '\u{FEFF}')
}

/// Trim the way JavaScript's `String.prototype.trim` does.
pub fn js_trim(text: &str) -> &str {
    text.trim_matches(is_js_whitespace)
}

/// Trim in place, without allocating a copy.
///
/// The content of every file is trimmed, so this saves a copy of the whole
/// repository.
pub fn js_trim_in_place(text: &mut String) {
    // The trailing whitespace.
    let end = text.trim_end_matches(is_js_whitespace).len();
    // The leading whitespace.
    let start = text.len() - text.trim_start_matches(is_js_whitespace).len();
    if start > 0 {
        text.drain(..start);
    }
    // `end` was measured from the old beginning of the string.
    let end = end.saturating_sub(start);
    if end < text.len() {
        text.truncate(end);
    }
}

/// Trim in place and append the trailing newline, without allocating a copy.
///
/// This is the allocation free equivalent of `format!("{}\n", js_trim(&out))`.
pub fn finalize_trimmed_output(mut out: String) -> String {
    // Trailing whitespace.
    let end = out.trim_end_matches(is_js_whitespace).len();
    out.truncate(end);
    // Leading whitespace.
    let start = out.len() - out.trim_start_matches(is_js_whitespace).len();
    if start > 0 {
        out.drain(..start);
    }
    out.push('\n');
    out
}

/// Estimate the size of the rendered output, so the buffer does not have to
/// grow (and copy) while it is filled.
pub fn estimate_output_size(
    files: &[crate::output::ProcessedFile],
    tree_string: &str,
    extra: usize,
) -> usize {
    let files_bytes: usize = files
        .iter()
        .map(|f| f.path.len() + f.content.len() + 64)
        .sum();
    files_bytes + tree_string.len() + extra + 8192
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_extensions() {
        assert_eq!(language_from_file_path("src/main.rs"), "rust");
        assert_eq!(language_from_file_path("a/b/c.ts"), "typescript");
        assert_eq!(language_from_file_path("Makefile"), "");
        assert_eq!(language_from_file_path("weird.xyz"), "");
    }

    #[test]
    fn escapes_xml() {
        assert_eq!(escape_xml("a & b < c"), "a &amp; b &lt; c");
    }
}
