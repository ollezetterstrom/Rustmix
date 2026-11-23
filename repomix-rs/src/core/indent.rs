//! Indentation transformations (4 spaces to tab).

/// Convert 4-space leading indentation to `\t` (1 token per indent level).
pub fn convert_indent_to_tabs(content: &str) -> String {
    let mut out = String::with_capacity(content.len());
    let mut first = true;
    for line in content.split('\n') {
        if !first {
            out.push('\n');
        }
        first = false;

        let bytes = line.as_bytes();
        let mut i = 0;
        let mut spaces = 0;
        while i < bytes.len() {
            if bytes[i] == b' ' {
                spaces += 1;
                if spaces == 4 {
                    out.push('\t');
                    spaces = 0;
                }
                i += 1;
            } else if bytes[i] == b'\t' {
                for _ in 0..spaces {
                    out.push(' ');
                }
                spaces = 0;
                out.push('\t');
                i += 1;
            } else {
                break;
            }
        }
        for _ in 0..spaces {
            out.push(' ');
        }
        out.push_str(&line[i..]);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_four_spaces_to_tabs() {
        let input = "    fn foo() {\n        let x = 1;\n    }";
        let expected = "\tfn foo() {\n\t\tlet x = 1;\n\t}";
        assert_eq!(convert_indent_to_tabs(input), expected);
    }

    #[test]
    fn handles_mixed_and_partial_indents() {
        let input = "      fn bar() {\n  let y = 2;\n}";
        let expected = "\t  fn bar() {\n  let y = 2;\n}";
        assert_eq!(convert_indent_to_tabs(input), expected);
    }
}
