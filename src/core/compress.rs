//! Code compression (`--compress`).
//!
//! Uses tree-sitter to extract the structural skeleton of a source file:
//! imports, types, interfaces, classes and function signatures. Method bodies
//! are dropped.

use std::cell::RefCell;
use std::collections::HashSet;

use tree_sitter::{Language as TsLanguage, Parser, Query, QueryCursor, StreamingIterator};

/// The separator placed between two compressed chunks.
pub const CHUNK_SEPARATOR: &str = "\u{22EE}----";

/// Supported languages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    JavaScript,
    TypeScript,
    Tsx,
    Python,
    Go,
    Rust,
    Java,
    CSharp,
    Ruby,
    Php,
    Swift,
    C,
    Cpp,
    Css,
    Solidity,
    Vue,
    Dart,
}

/// The parse strategy used for a language.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Strategy {
    Default,
    TypeScript,
    Python,
    Go,
    Css,
    Vue,
}

impl Language {
    /// Guess the language from a file path.
    pub fn guess(path: &str) -> Option<Language> {
        let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
        let ext = name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase())?;
        let lang = match ext.as_str() {
            "js" | "jsx" | "cjs" | "mjs" | "mjsx" => Language::JavaScript,
            "ts" | "mts" | "cts" => Language::TypeScript,
            "tsx" | "mtsx" => Language::Tsx,
            "py" => Language::Python,
            "go" => Language::Go,
            "rs" => Language::Rust,
            "java" => Language::Java,
            "cs" => Language::CSharp,
            "rb" => Language::Ruby,
            "php" => Language::Php,
            "swift" => Language::Swift,
            "c" | "h" => Language::C,
            "cpp" | "hpp" => Language::Cpp,
            "css" => Language::Css,
            "sol" => Language::Solidity,
            "vue" => Language::Vue,
            "dart" => Language::Dart,
            _ => return None,
        };
        Some(lang)
    }

    fn strategy(self) -> Strategy {
        match self {
            Language::JavaScript | Language::TypeScript | Language::Tsx => Strategy::TypeScript,
            Language::Python => Strategy::Python,
            Language::Go => Strategy::Go,
            Language::Css => Strategy::Css,
            Language::Vue => Strategy::Vue,
            _ => Strategy::Default,
        }
    }

    fn ts_language(self) -> Option<TsLanguage> {
        let lang = match self {
            Language::JavaScript => tree_sitter_javascript::LANGUAGE.into(),
            Language::TypeScript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            Language::Tsx => tree_sitter_typescript::LANGUAGE_TSX.into(),
            Language::Python => tree_sitter_python::LANGUAGE.into(),
            Language::Go => tree_sitter_go::LANGUAGE.into(),
            Language::Rust => tree_sitter_rust::LANGUAGE.into(),
            Language::Java => tree_sitter_java::LANGUAGE.into(),
            Language::CSharp => tree_sitter_c_sharp::LANGUAGE.into(),
            Language::Ruby => tree_sitter_ruby::LANGUAGE.into(),
            Language::Php => tree_sitter_php::LANGUAGE_PHP.into(),
            Language::Swift => tree_sitter_swift::LANGUAGE.into(),
            Language::C => tree_sitter_c::LANGUAGE.into(),
            Language::Cpp => tree_sitter_cpp::LANGUAGE.into(),
            Language::Css => tree_sitter_css::LANGUAGE.into(),
            Language::Solidity => tree_sitter_solidity::LANGUAGE.into(),
            Language::Vue => return None,
            Language::Dart => tree_sitter_dart::LANGUAGE.into(),
        };
        Some(lang)
    }

    fn query_source(self) -> &'static str {
        match self {
            Language::JavaScript => QUERY_JAVASCRIPT,
            Language::TypeScript | Language::Tsx => QUERY_TYPESCRIPT,
            Language::Python => QUERY_PYTHON,
            Language::Go => QUERY_GO,
            Language::Rust => QUERY_RUST,
            Language::Java => QUERY_JAVA,
            Language::CSharp => QUERY_CSHARP,
            Language::Ruby => QUERY_RUBY,
            Language::Php => QUERY_PHP,
            Language::Swift => QUERY_SWIFT,
            Language::C => QUERY_C,
            Language::Cpp => QUERY_CPP,
            Language::Css => QUERY_CSS,
            Language::Solidity => QUERY_SOLIDITY,
            Language::Vue => QUERY_VUE,
            Language::Dart => QUERY_DART,
        }
    }
}

// ---------------------------------------------------------------------------
// Queries
// ---------------------------------------------------------------------------

const QUERY_RUST: &str = r#"
(line_comment) @comment
(block_comment) @comment
(use_declaration
  (scoped_identifier) @name.reference.module) @definition.import
(use_declaration
  (identifier) @name.reference.module) @definition.import
(extern_crate_declaration
  (identifier) @name.reference.module) @definition.import
(struct_item
    name: (type_identifier) @name.definition.class) @definition.class
(enum_item
    name: (type_identifier) @name.definition.class) @definition.class
(union_item
    name: (type_identifier) @name.definition.class) @definition.class
(type_item
    name: (type_identifier) @name.definition.class) @definition.class
(declaration_list
    (function_item
        name: (identifier) @name.definition.method)) @definition.method
(function_item
    name: (identifier) @name.definition.function) @definition.function
(trait_item
    name: (type_identifier) @name.definition.interface) @definition.interface
(mod_item
    name: (identifier) @name.definition.module) @definition.module
(macro_definition
    name: (identifier) @name.definition.macro) @definition.macro
(call_expression
    function: (identifier) @name.reference.call) @reference.call
(call_expression
    function: (field_expression
      field: (field_identifier) @name.reference.call)) @reference.call
(macro_invocation
    macro: (identifier) @name.reference.call) @reference.call
(impl_item
    trait: (type_identifier) @name.reference.implementation) @reference.implementation
(impl_item
    type: (type_identifier) @name.reference.implementation
    !trait) @reference.implementation
"#;

const QUERY_JAVASCRIPT: &str = r#"
(comment) @comment
(
  (comment)* @doc
  .
  (method_definition
    name: (property_identifier) @name.definition.method) @definition.method
)
(
  (comment)* @doc
  .
  [
    (class
      name: (_) @name.definition.class)
    (class_declaration
      name: (_) @name.definition.class)
  ] @definition.class
)
(
  (comment)* @doc
  .
  [
    (function_declaration
      name: (identifier) @name.definition.function)
    (generator_function
      name: (identifier) @name.definition.function)
    (generator_function_declaration
      name: (identifier) @name.definition.function)
  ] @definition.function
)
(
  (comment)* @doc
  .
  (lexical_declaration
    (variable_declarator
      name: (identifier) @name.definition.function
      value: [(arrow_function) (function_declaration)]) @definition.function)
)
(
  (comment)* @doc
  .
  (variable_declaration
    (variable_declarator
      name: (identifier) @name.definition.function
      value: [(arrow_function) (function_declaration)]) @definition.function)
)
(assignment_expression
  left: [
    (identifier) @name.definition.function
    (member_expression
      property: (property_identifier) @name.definition.function)
  ]
  right: [(arrow_function) (function_declaration)]
) @definition.function
(pair
  key: (property_identifier) @name.definition.function
  value: [(arrow_function) (function_declaration)]) @definition.function
(
  (call_expression
    function: (identifier) @name.reference.call) @reference.call
)
(call_expression
  function: (member_expression
    property: (property_identifier) @name.reference.call)
    arguments: (_) @reference.call)
(new_expression
  constructor: (_) @name.reference.class) @reference.class
"#;

const QUERY_TYPESCRIPT: &str = r#"
(import_statement
  (import_clause (identifier) @name.reference.module)) @definition.import
(import_statement
  (import_clause
    (named_imports
      (import_specifier
        name: (identifier) @name.reference.module))) @definition.import)
(comment) @comment
(function_signature
  name: (identifier) @name.definition.function) @definition.function
(method_signature
  name: (property_identifier) @name.definition.method) @definition.method
(abstract_method_signature
  name: (property_identifier) @name.definition.method) @definition.method
(abstract_class_declaration
  name: (type_identifier) @name.definition.class) @definition.class
(module
  name: (identifier) @name.definition.module) @definition.module
(interface_declaration
  name: (type_identifier) @name.definition.interface) @definition.interface
(type_annotation
  (type_identifier) @name.reference.type) @reference.type
(new_expression
  constructor: (identifier) @name.reference.class) @reference.class
(function_declaration
  name: (identifier) @name.definition.function) @definition.function
(method_definition
  name: (property_identifier) @name.definition.method) @definition.method
(class_declaration
  name: (type_identifier) @name.definition.class) @definition.class
(interface_declaration
  name: (type_identifier) @name.definition.class) @definition.class
(type_alias_declaration
  name: (type_identifier) @name.definition.type) @definition.type
(enum_declaration
  name: (identifier) @name.definition.enum) @definition.enum
(lexical_declaration
    (variable_declarator
      name: (identifier) @name.definition.function
      value: (arrow_function)
    )
  ) @definition.function
(variable_declaration
    (variable_declarator
      name: (identifier) @name.definition.function
      value: (arrow_function)
    )
) @definition.function
(assignment_expression
    left: [(identifier) @name.definition.function]
    right: (arrow_function)
) @definition.function
"#;

const QUERY_PYTHON: &str = r#"
(comment) @comment
(expression_statement
  (string) @comment) @docstring
(import_statement
  name: (dotted_name) @name.reference.module) @definition.import
(import_from_statement
  module_name: (dotted_name) @name.reference.module) @definition.import
(import_from_statement
  name: (dotted_name) @name.reference.module) @definition.import
(class_definition
  name: (identifier) @name.definition.class) @definition.class
(function_definition
  name: (identifier) @name.definition.function) @definition.function
(call
  function: [
      (identifier) @name.reference.call
      (attribute
        attribute: (identifier) @name.reference.call)
  ]) @reference.call
(assignment
  left: (identifier) @name.definition.type_alias) @definition.type_alias
"#;

const QUERY_GO: &str = r#"
(comment) @comment
(package_clause) @definition.package
(import_declaration) @definition.import
(import_spec) @definition.import
(var_declaration) @definition.variable
(const_declaration) @definition.constant
(
  (comment)* @doc
  .
  (function_declaration
    name: (identifier) @name) @definition.function
)
(
  (comment)* @doc
  .
  (method_declaration
    name: (field_identifier) @name) @definition.method
)
(call_expression
  function: [
    (identifier) @name
    (parenthesized_expression (identifier) @name)
    (selector_expression field: (field_identifier) @name)
    (parenthesized_expression (selector_expression field: (field_identifier) @name))
  ]) @reference.call
(type_spec
  name: (type_identifier) @name) @definition.type
(type_identifier) @name @reference.type
(package_clause "package" (package_identifier) @name)
(type_declaration (type_spec name: (type_identifier) @name type: (interface_type)))
(type_declaration (type_spec name: (type_identifier) @name type: (struct_type)))
(import_declaration
  (import_spec_list
    (import_spec
      path: (interpreted_string_literal) @name.reference.module))) @definition.import
(import_declaration
  (import_spec
    path: (interpreted_string_literal) @name.reference.module)) @definition.import
(package_clause
  (package_identifier) @name.reference.module) @definition.package
(var_declaration (var_spec name: (identifier) @name))
(const_declaration (const_spec name: (identifier) @name))
"#;

const QUERY_JAVA: &str = r#"
(line_comment) @comment
(block_comment) @comment
(import_declaration
  .
  (identifier) @name.reference.module) @definition.import
(package_declaration
  .
  (identifier) @name.reference.module) @definition.import
(class_declaration
  name: (identifier) @name.definition.class) @definition.class
(method_declaration
  name: (identifier) @name.definition.method) @definition.method
(method_invocation
  name: (identifier) @name.reference.call
  arguments: (argument_list) @reference.call)
(interface_declaration
  name: (identifier) @name.definition.interface) @definition.interface
(type_list
  (type_identifier) @name.reference.implementation) @reference.implementation
(object_creation_expression
  type: (type_identifier) @name.reference.class) @reference.class
(superclass (type_identifier) @name.reference.class) @reference.class
"#;

const QUERY_CSHARP: &str = r#"
(comment) @comment
(class_declaration
  name: (identifier) @name.definition.class
) @definition.class
(class_declaration
  (base_list (_) @name.reference.class)
) @reference.class
(interface_declaration
  name: (identifier) @name.definition.interface
) @definition.interface
(interface_declaration
  (base_list (_) @name.reference.interface)
) @reference.interface
(method_declaration
  name: (identifier) @name.definition.method
) @definition.method
(object_creation_expression
  type: (identifier) @name.reference.class
) @reference.class
(type_parameter_constraints_clause
  (identifier) @name.reference.class
) @reference.class
(type_parameter_constraint
  type: (identifier) @name.reference.class
) @reference.class
(variable_declaration
  type: (identifier) @name.reference.class
) @reference.class
(invocation_expression
  function:
    (member_access_expression
      name: (identifier) @name.reference.send
    )
) @reference.send
(namespace_declaration
  name: (identifier) @name.definition.module
) @definition.module
"#;

const QUERY_RUBY: &str = r#"
(comment) @comment
(call
  (identifier) @name.reference.module) @definition.import
(
  (comment)* @doc
  .
  [
    (method
      name: (_) @name.definition.method) @definition.method
    (singleton_method
      name: (_) @name.definition.method) @definition.method
  ]
)
(alias
  name: (_) @name.definition.method) @definition.method
(setter
  (identifier) @ignore)
(
  (comment)* @doc
  .
  [
    (class
      name: [
        (constant) @name.definition.class
        (scope_resolution
          name: (_) @name.definition.class)
      ]) @definition.class
    (singleton_class
      value: [
        (constant) @name.definition.class
        (scope_resolution
          name: (_) @name.definition.class)
      ]) @definition.class
  ]
)
(
  (module
    name: [
      (constant) @name.definition.module
      (scope_resolution
        name: (_) @name.definition.module)
    ]) @definition.module
)
(call method: (identifier) @name.reference.call) @reference.call
([(identifier) (constant)] @name.reference.call @reference.call)
"#;

const QUERY_PHP: &str = r#"
(comment) @comment
(namespace_use_clause) @definition.import
(enum_declaration name: (name) @name) @definition.enum
(namespace_definition
  name: (namespace_name) @name) @definition.module
(interface_declaration
  name: (name) @name) @definition.interface
(trait_declaration
  name: (name) @name) @definition.interface
(class_declaration
  name: (name) @name) @definition.class
(class_interface_clause [(name) (qualified_name)] @name) @reference.implementation
(property_declaration
  (property_element (variable_name (name) @name))) @definition.field
(function_definition
  name: (name) @name) @definition.function
(method_declaration
  name: (name) @name) @definition.function
(object_creation_expression
  [
    (qualified_name (name) @name)
    (variable_name (name) @name)
  ]) @reference.class
(function_call_expression
  function: [
    (qualified_name (name) @name)
    (variable_name (name)) @name
  ]) @reference.call
(scoped_call_expression
  name: (name) @name) @reference.call
(member_call_expression
  name: (name) @name) @reference.call
"#;

const QUERY_SWIFT: &str = r#"
(comment) @comment
(class_declaration
  name: (type_identifier) @name) @definition.class
(protocol_declaration
  name: (type_identifier) @name) @definition.interface
(class_body
  [
    (function_declaration
      name: (simple_identifier) @name
    ) @definition.method
    (subscript_declaration) @name.definition.method
    (init_declaration "init" @name) @definition.method
    (deinit_declaration "deinit" @name) @definition.method
  ]
)
(protocol_body
  [
    (protocol_function_declaration
      name: (simple_identifier) @name
    ) @definition.method
    (subscript_declaration) @name.definition.method
    (init_declaration "init" @name) @definition.method
  ]
)
(class_body
  [
    (property_declaration
      (pattern (simple_identifier) @name)
    ) @definition.property
  ]
)
(property_declaration
    (pattern (simple_identifier) @name)
) @definition.property
(function_declaration
    name: (simple_identifier) @name) @definition.function
"#;

const QUERY_C: &str = r#"
(comment) @comment
(struct_specifier name: (type_identifier) @name.definition.class body:(_)) @definition.class
(declaration type: (union_specifier name: (type_identifier) @name.definition.class)) @definition.class
(function_declarator declarator: (identifier) @name.definition.function) @definition.function
(type_definition declarator: (type_identifier) @name.definition.type) @definition.type
(enum_specifier name: (type_identifier) @name.definition.type) @definition.type
"#;

const QUERY_CPP: &str = r#"
(comment) @comment
(struct_specifier name: (type_identifier) @name.definition.class body:(_)) @definition.class
(declaration type: (union_specifier name: (type_identifier) @name.definition.class)) @definition.class
(function_declarator declarator: (identifier) @name.definition.function) @definition.function
(function_declarator declarator: (field_identifier) @name.definition.function) @definition.function
(function_declarator declarator: (qualified_identifier scope: (namespace_identifier) @scope name: (identifier) @name.definition.method)) @definition.method
(type_definition declarator: (type_identifier) @name.definition.type) @definition.type
(enum_specifier name: (type_identifier) @name.definition.type) @definition.type
(class_specifier name: (type_identifier) @name.definition.class) @definition.class
"#;

const QUERY_CSS: &str = r#"
(comment) @comment
(rule_set
  (selectors) @name.definition.selector
) @definition.selector
(at_rule) @definition.at_rule
"#;

const QUERY_SOLIDITY: &str = r#"
(comment) @comment
(contract_declaration
  name: (identifier) @name.definition.class) @definition.class
(interface_declaration
  name: (identifier) @name.definition.interface) @definition.interface
(function_definition
  name: (identifier) @name.definition.function) @definition.function
(import_directive) @definition.import
(event_definition
  name: (identifier) @name.definition.event) @definition.event
(modifier_definition
  name: (identifier) @name.definition.modifier) @definition.modifier
"#;

const QUERY_VUE: &str = r#"
(comment) @comment
(template_element) @template
(script_element) @script
(style_element) @style
(interpolation) @interpolation
"#;

const QUERY_DART: &str = r#"
(comment) @comment
(documentation_comment) @comment
(import_or_export) @definition.import
(class_definition
  name: (identifier) @name.definition.class) @definition.class
(mixin_declaration
  (identifier) @name.definition.mixin) @definition.mixin
(enum_declaration
  name: (identifier) @name.definition.enum) @definition.enum
(extension_declaration
  name: (identifier) @name.definition.extension) @definition.extension
(type_alias
  (type_identifier) @name.definition.type) @definition.type
(function_signature
  name: (identifier) @name.definition.function) @definition.function
(getter_signature
  name: (identifier) @name.definition.method) @definition.method
(setter_signature
  name: (identifier) @name.definition.method) @definition.method
(method_signature
 (constructor_signature) @name.definition.method) @definition.method
(declaration
 (constructor_signature) @name.definition.method) @definition.method
(declaration
 (constant_constructor_signature) @name.definition.method) @definition.method
(method_signature
 (operator_signature) @name.definition.method) @definition.method
(method_signature
 (factory_constructor_signature) @name.definition.method) @definition.method
(declaration
 (factory_constructor_signature) @name.definition.method) @definition.method
(declaration
 (redirecting_factory_constructor_signature) @name.definition.method) @definition.method
"#;

// ---------------------------------------------------------------------------
// Compression
// ---------------------------------------------------------------------------

thread_local! {
    static PARSER: RefCell<Parser> = RefCell::new(Parser::new());
}

/// Compress a file's content. Returns `None` when the language is unsupported
/// or the file cannot be parsed.
pub fn compress_file(path: &str, content: &str) -> Option<String> {
    let language = Language::guess(path)?;
    compress_with_language(language, content)
}

/// Compress content using a specific language.
pub fn compress_with_language(language: Language, content: &str) -> Option<String> {
    let ts_language = language.ts_language()?;
    let query = Query::new(&ts_language, language.query_source()).ok()?;

    let tree = PARSER.with(|p| {
        let mut parser = p.borrow_mut();
        parser.set_language(&ts_language).ok()?;
        parser.parse(content, None)
    })?;

    let lines: Vec<&str> = content.split('\n').collect();

    // Collect the captures.
    let mut captures: Vec<(usize, usize, String)> = Vec::new();
    let mut cursor = QueryCursor::new();
    let mut it = cursor.captures(&query, tree.root_node(), content.as_bytes());
    while let Some((m, capture_index)) = it.next() {
        let capture = m.captures()[*capture_index];
        let name = query.capture_names()[capture.index as usize].to_string();
        let node = capture.node;
        captures.push((
            node.start_position().row,
            node.end_position().row,
            name,
        ));
    }
    drop(it);
    drop(cursor);

    // Sort by start row, keeping the original order for ties.
    captures.sort_by_key(|c| c.0);

    let strategy = language.strategy();
    let mut processed: HashSet<String> = HashSet::new();
    let mut chunks: Vec<Chunk> = Vec::new();

    for (start_row, end_row, name) in &captures {
        let Some(content) = parse_capture(strategy, &name, &lines, *start_row, *end_row, &mut processed)
        else {
            continue;
        };
        chunks.push(Chunk {
            content: content.trim().to_string(),
            start_row: *start_row,
            end_row: *end_row,
        });
    }

    let filtered = filter_duplicated_chunks(chunks);
    let merged = merge_adjacent_chunks(filtered);

    Some(
        merged
            .iter()
            .map(|c| c.content.clone())
            .collect::<Vec<_>>()
            .join(&format!("\n{CHUNK_SEPARATOR}\n"))
            .trim()
            .to_string(),
    )
}

/// A compressed chunk of source code.
struct Chunk {
    content: String,
    start_row: usize,
    end_row: usize,
}

/// Handle a single capture according to the language strategy.
fn parse_capture(
    strategy: Strategy,
    name: &str,
    lines: &[&str],
    start_row: usize,
    end_row: usize,
    processed: &mut HashSet<String>,
) -> Option<String> {
    if start_row >= lines.len() {
        return None;
    }
    match strategy {
        Strategy::Default => {
            let selected = extract_lines(lines, start_row, end_row)?;
            let is_name = name.contains("name");
            let is_comment = name.contains("comment");
            let is_import = name.contains("import") || name.contains("require");
            if !(is_name || is_comment || is_import) {
                return None;
            }
            let chunk = selected.join("\n");
            if !check_and_add(chunk.clone(), processed) {
                return None;
            }
            Some(chunk)
        }
        Strategy::TypeScript => parse_typescript_capture(name, lines, start_row, end_row, processed),
        Strategy::Python => parse_python_capture(name, lines, start_row, end_row, processed),
        Strategy::Go => parse_go_capture(name, lines, start_row, end_row, processed),
        Strategy::Css => {
            let is_comment = name.contains("comment");
            let is_selector = name.contains("selector") || name.contains("definition.selector");
            let is_at_rule = name.contains("at_rule") || name.contains("definition.at_rule");
            if !(is_comment || is_selector || is_at_rule) {
                return None;
            }
            let selected: Vec<&str> = if is_comment {
                lines[start_row..=(end_row.min(lines.len() - 1))].to_vec()
            } else {
                vec![lines[start_row]]
            };
            if selected.is_empty() {
                return None;
            }
            let chunk = selected.join("\n");
            if !check_and_add(chunk.clone(), processed) {
                return None;
            }
            Some(chunk)
        }
        Strategy::Vue => {
            let selected = extract_lines(lines, start_row, end_row)?;
            let chunk = selected.join("\n");
            let chunk_id = format!("{name}:{start_row}");
            if processed.contains(&chunk_id) {
                return None;
            }
            processed.insert(chunk_id);
            Some(chunk)
        }
    }
}

fn extract_lines<'a>(lines: &'a [&'a str], start_row: usize, end_row: usize) -> Option<Vec<&'a str>> {
    if start_row >= lines.len() {
        return None;
    }
    let end = end_row.min(lines.len() - 1);
    let selected = &lines[start_row..=end];
    if selected.is_empty() {
        None
    } else {
        Some(selected.to_vec())
    }
}

fn check_and_add(content: String, processed: &mut HashSet<String>) -> bool {
    let normalized = content.trim().to_string();
    if processed.contains(&normalized) {
        return false;
    }
    processed.insert(normalized);
    true
}

// --- TypeScript / JavaScript ---------------------------------------------

const TS_CAPTURE_TYPES: [&str; 9] = [
    "comment",
    "definition.interface",
    "definition.type",
    "definition.enum",
    "definition.class",
    "definition.import",
    "definition.function",
    "definition.method",
    "definition.property",
];

fn capture_types(name: &str) -> HashSet<&'static str> {
    let mut types = HashSet::new();
    for t in TS_CAPTURE_TYPES {
        if name.contains(t) {
            types.insert(t);
        }
    }
    types
}

fn parse_typescript_capture(
    name: &str,
    lines: &[&str],
    start_row: usize,
    end_row: usize,
    processed: &mut HashSet<String>,
) -> Option<String> {
    let types = capture_types(name);
    if types.contains("definition.function") || types.contains("definition.method") {
        return parse_ts_function(lines, start_row, end_row, processed);
    }
    if types.contains("definition.class") {
        return parse_ts_class(lines, start_row, end_row, processed);
    }
    if types.contains("definition.interface")
        || types.contains("definition.type")
        || types.contains("definition.enum")
        || types.contains("definition.import")
    {
        return parse_ts_type_or_import(lines, start_row, end_row, processed);
    }
    if types.contains("comment") {
        let selected = extract_lines(lines, start_row, end_row)?;
        return Some(selected.join("\n").trim().to_string());
    }
    None
}

/// Match `(?:export\s+)?(?:const|let|var)\s+([a-zA-Z0-9_$]+)\s*=`.
fn ts_function_name(line: &str) -> Option<String> {
    let bytes = line.as_bytes();
    let mut i = 0usize;
    // Optional "export".
    let rest = if line[8..].trim_start().starts_with("const")
        || line[8..].trim_start().starts_with("let")
        || line[8..].trim_start().starts_with("var")
    {
        if !line[..8].trim().is_empty() {
            return None;
        }
        &line[8..]
    } else {
        line
    };
    let _ = bytes;
    let trimmed = rest.trim_start();
    let mut chars = trimmed.char_indices();
    let mut idx = 0usize;
    for kw in ["const", "let", "var"] {
        if trimmed.starts_with(kw) {
            idx = kw.len();
            break;
        }
    }
    if idx == 0 {
        return None;
    }
    let after = &trimmed[idx..];
    let after = after.trim_start();
    let name: String = after
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '$')
        .collect();
    if name.is_empty() {
        return None;
    }
    let mut it = after[name.len()..].trim_start().chars();
    match it.next() {
        Some('=') => Some(name),
        _ => {
            let _ = &mut chars;
            None
        }
    }
}

fn parse_ts_function(
    lines: &[&str],
    start_row: usize,
    end_row: usize,
    processed: &mut HashSet<String>,
) -> Option<String> {
    let function_name = ts_function_name(lines[start_row]);
    if let Some(fname) = &function_name {
        if processed.contains(&format!("func:{fname}")) {
            return None;
        }
    }
    let signature_end = find_ts_signature_end(lines, start_row, end_row);
    let selected = &lines[start_row..=signature_end.min(lines.len() - 1)];
    let cleaned = clean_ts_signature(selected);
    if !check_and_add(cleaned.clone(), processed) {
        return None;
    }
    if let Some(fname) = function_name {
        processed.insert(format!("func:{fname}"));
    }
    Some(cleaned)
}

fn find_ts_signature_end(lines: &[&str], start_row: usize, end_row: usize) -> usize {
    let end = end_row.min(lines.len() - 1);
    for i in start_row..=end {
        let line = lines[i].trim();
        if line.contains(')')
            && (line.ends_with('{') || line.ends_with("=>") || line.ends_with(';'))
        {
            return i;
        }
    }
    start_row
}

fn clean_ts_signature(lines: &[&str]) -> String {
    let mut result: Vec<String> = lines.iter().map(|s| s.to_string()).collect();
    if let Some(last) = result.last_mut() {
        if let Some(idx) = last.find('{') {
            *last = last[..idx].trim().to_string();
        } else if let Some(idx) = last.find("=>") {
            *last = last[..idx].trim().to_string();
        }
    }
    result.join("\n").trim().to_string()
}

fn parse_ts_class(
    lines: &[&str],
    start_row: usize,
    end_row: usize,
    processed: &mut HashSet<String>,
) -> Option<String> {
    let mut selected = vec![lines[start_row].to_string()];
    if start_row + 1 <= end_row && start_row + 1 < lines.len() {
        let next = lines[start_row + 1].trim();
        if next.contains("extends") || next.contains("implements") {
            selected.push(next.to_string());
        }
    }
    let cleaned: Vec<String> = selected
        .iter()
        .map(|l| {
            match l.find('{') {
                Some(i) => l[..i].trim().to_string(),
                None => l.trim().to_string(),
            }
        })
        .collect();
    let definition = cleaned.join("\n").trim().to_string();
    if !check_and_add(definition.clone(), processed) {
        return None;
    }
    Some(definition)
}

fn parse_ts_type_or_import(
    lines: &[&str],
    start_row: usize,
    end_row: usize,
    processed: &mut HashSet<String>,
) -> Option<String> {
    let selected = extract_lines(lines, start_row, end_row)?;
    let definition = selected.join("\n").trim().to_string();
    if !check_and_add(definition.clone(), processed) {
        return None;
    }
    Some(definition)
}

// --- Python ---------------------------------------------------------------

const PY_CAPTURE_TYPES: [&str; 5] = [
    "comment",
    "definition.class",
    "definition.function",
    "docstring",
    "definition.type_alias",
];

fn parse_python_capture(
    name: &str,
    lines: &[&str],
    start_row: usize,
    end_row: usize,
    processed: &mut HashSet<String>,
) -> Option<String> {
    let mut types = HashSet::new();
    for t in PY_CAPTURE_TYPES {
        if name.contains(t) {
            types.insert(t);
        }
    }
    if types.contains("definition.class") {
        let decorators = python_decorators(lines, start_row);
        let class_def = python_class_line(lines[start_row]);
        let full = decorators
            .iter()
            .cloned()
            .chain(std::iter::once(class_def))
            .collect::<Vec<_>>()
            .join("\n");
        if !check_and_add(full.clone(), processed) {
            return None;
        }
        return Some(full);
    }
    if types.contains("definition.function") {
        let decorators = python_decorators(lines, start_row);
        let Some(signature) = python_function_signature(lines[start_row]) else {
            return None;
        };
        let full = decorators
            .iter()
            .cloned()
            .chain(std::iter::once(signature))
            .collect::<Vec<_>>()
            .join("\n");
        if !check_and_add(full.clone(), processed) {
            return None;
        }
        return Some(full);
    }
    if types.contains("docstring") || types.contains("comment") {
        let selected = extract_lines(lines, start_row, end_row)?;
        let content = selected.join("\n");
        if !check_and_add(content.clone(), processed) {
            return None;
        }
        return Some(content);
    }
    if types.contains("definition.type_alias") {
        let alias = lines[start_row].trim().to_string();
        if !check_and_add(alias.clone(), processed) {
            return None;
        }
        return Some(alias);
    }
    None
}

fn python_decorators(lines: &[&str], start_row: usize) -> Vec<String> {
    let mut decorators = Vec::new();
    let mut current = start_row as isize - 1;
    while current >= 0 {
        let line = lines[current as usize].trim();
        if line.starts_with('@') {
            decorators.insert(0, line.to_string());
        } else {
            break;
        }
        current -= 1;
    }
    decorators
}

fn python_class_line(line: &str) -> String {
    // `class\s+\w+\s*\((.*?)\):` â€” the result is the same either way, the
    // trailing colon is simply removed.
    strip_trailing_colon(line)
}

fn python_function_signature(line: &str) -> Option<String> {
    // `def\s+(\w+)\s*\((.*?)\)(\s*->\s*[^:]+)?:`
    let trimmed = line.trim_start();
    if !trimmed.starts_with("def") {
        return None;
    }
    let after = trimmed[3..].trim_start();
    let mut chars = after.char_indices();
    let mut name_len = 0usize;
    for (i, c) in chars.by_ref() {
        if c.is_ascii_alphanumeric() || c == '_' {
            name_len = i + c.len_utf8();
        } else {
            name_len = i;
            break;
        }
    }
    if name_len == 0 {
        return None;
    }
    let rest = &after[name_len..];
    let rest_trim = rest.trim_start();
    if !rest_trim.starts_with('(') {
        return None;
    }
    // Find the matching closing parenthesis and the trailing colon.
    let open = rest_trim.find('(')?;
    let mut depth = 0usize;
    let mut close = None;
    for (i, c) in rest_trim[open..].char_indices() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    close = Some(open + i);
                    break;
                }
            }
            _ => {}
        }
    }
    let close = close?;
    let after_close = &rest_trim[close + 1..];
    // Optional return annotation.
    let after_close_trim = after_close.trim_start();
    let mut tail = after_close_trim;
    if let Some(stripped) = tail.strip_prefix("->") {
        let mut end = 0usize;
        for (i, c) in stripped.char_indices() {
            if c == ':' {
                end = i;
                break;
            }
            end = i + c.len_utf8();
        }
        tail = &stripped[..end];
    } else {
        tail = "";
    }
    let _ = tail;
    if !after_close_trim.contains(':') {
        return None;
    }
    Some(strip_trailing_colon(line))
}

fn strip_trailing_colon(line: &str) -> String {
    let trimmed = line.trim_end();
    if let Some(stripped) = trimmed.strip_suffix(':') {
        stripped.trim_end().to_string()
    } else {
        trimmed.to_string()
    }
}

// --- Go -------------------------------------------------------------------

const GO_CAPTURE_TYPES: [&str; 11] = [
    "comment",
    "definition.type",
    "definition.interface",
    "definition.struct",
    "definition.package",
    "definition.import",
    "definition.function",
    "definition.method",
    "definition.module",
    "definition.variable",
    "definition.constant",
];

fn parse_go_capture(
    name: &str,
    lines: &[&str],
    start_row: usize,
    end_row: usize,
    processed: &mut HashSet<String>,
) -> Option<String> {
    let mut types = HashSet::new();
    for t in GO_CAPTURE_TYPES {
        if name.contains(t) {
            types.insert(t);
        }
    }
    if types.contains("comment") {
        return parse_go_block(lines, start_row, end_row, processed);
    }
    if types.contains("definition.package") || types.contains("definition.module") {
        return parse_go_simple(lines, start_row, processed);
    }
    if types.contains("definition.import") {
        return if lines[start_row].contains('(') {
            parse_go_block(lines, start_row, end_row, processed)
        } else {
            parse_go_simple(lines, start_row, processed)
        };
    }
    if types.contains("definition.variable") || types.contains("definition.constant") {
        return parse_go_block(lines, start_row, end_row, processed);
    }
    if types.contains("definition.type")
        || types.contains("definition.interface")
        || types.contains("definition.struct")
    {
        return parse_go_type(lines, start_row, end_row, processed);
    }
    if types.contains("definition.function") {
        return parse_go_function(lines, start_row, end_row, processed, false);
    }
    if types.contains("definition.method") {
        return parse_go_function(lines, start_row, end_row, processed, true);
    }
    None
}

fn parse_go_simple(lines: &[&str], start_row: usize, processed: &mut HashSet<String>) -> Option<String> {
    let declaration = lines[start_row].trim().to_string();
    if !check_and_add(declaration.clone(), processed) {
        return None;
    }
    Some(declaration)
}

fn parse_go_block(
    lines: &[&str],
    start_row: usize,
    end_row: usize,
    processed: &mut HashSet<String>,
) -> Option<String> {
    let block_end = if lines[start_row].contains('(') {
        find_closing_token(lines, start_row, end_row, ')')
    } else {
        end_row
    };
    let selected = extract_lines(lines, start_row, block_end)?;
    let declaration = selected.join("\n");
    if !check_and_add(declaration.clone(), processed) {
        return None;
    }
    Some(declaration)
}

fn parse_go_function(
    lines: &[&str],
    start_row: usize,
    end_row: usize,
    processed: &mut HashSet<String>,
    is_method: bool,
) -> Option<String> {
    let name_key = if is_method { "method" } else { "func" };
    let name = if is_method {
        go_method_name(lines[start_row])
    } else {
        go_function_name(lines[start_row])
    };
    if let Some(n) = &name {
        if processed.contains(&format!("{name_key}:{n}")) {
            return None;
        }
    }
    let signature_end = find_closing_token(lines, start_row, end_row, '{');
    let selected = extract_lines(lines, start_row, signature_end)?;
    let signature = selected.join("\n").trim().to_string();
    let clean = match signature.find('{') {
        Some(i) => signature[..i].trim().to_string(),
        None => signature.trim().to_string(),
    };
    if !check_and_add(clean.clone(), processed) {
        return None;
    }
    if let Some(n) = name {
        processed.insert(format!("{name_key}:{n}"));
    }
    Some(clean)
}

fn parse_go_type(
    lines: &[&str],
    start_row: usize,
    end_row: usize,
    processed: &mut HashSet<String>,
) -> Option<String> {
    let signature_end = if lines[start_row].contains('{') {
        find_closing_token(lines, start_row, end_row, '}')
    } else {
        end_row
    };
    let selected = extract_lines(lines, start_row, signature_end)?;
    let definition = selected.join("\n");
    if !check_and_add(definition.clone(), processed) {
        return None;
    }
    Some(definition)
}

fn find_closing_token(lines: &[&str], start_row: usize, end_row: usize, token: char) -> usize {
    let end = end_row.min(lines.len() - 1);
    for i in start_row..=end {
        if lines[i].contains(token) {
            return i;
        }
    }
    start_row
}

fn go_function_name(line: &str) -> Option<String> {
    // `func\s+([A-Za-z0-9_]+)\s*\(`
    let idx = line.find("func")?;
    let rest = &line[idx + 4..];
    let trimmed = rest.trim_start();
    if !trimmed.starts_with('(') {
        let name: String = trimmed
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
            .collect();
        if name.is_empty() {
            return None;
        }
        let after = &trimmed[name.len()..];
        if after.trim_start().starts_with('(') {
            return Some(name);
        }
    }
    None
}

fn go_method_name(line: &str) -> Option<String> {
    // `func\s+\(([^)]+)\)\s+([A-Za-z0-9_]+)\s*\(`
    let idx = line.find("func")?;
    let rest = &line[idx + 4..];
    let trimmed = rest.trim_start();
    if !trimmed.starts_with('(') {
        return None;
    }
    let close = trimmed.find(')')?;
    let after = trimmed[close + 1..].trim_start();
    let name: String = after
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
        .collect();
    if name.is_empty() {
        return None;
    }
    let rest = &after[name.len()..];
    if rest.trim_start().starts_with('(') {
        Some(name)
    } else {
        None
    }
}

// --- Chunk post-processing ------------------------------------------------

fn filter_duplicated_chunks(chunks: Vec<Chunk>) -> Vec<Chunk> {
    let mut by_start: Vec<(usize, Vec<Chunk>)> = Vec::new();
    for chunk in chunks {
        match by_start.iter_mut().find(|(s, _)| *s == chunk.start_row) {
            Some((_, list)) => list.push(chunk),
            None => by_start.push((chunk.start_row, vec![chunk])),
        }
    }
    let mut out: Vec<Chunk> = Vec::new();
    for (_, mut list) in by_start {
        // Longest content first; ties keep the original order.
        list.sort_by(|a, b| b.content.len().cmp(&a.content.len()));
        out.push(list.remove(0));
    }
    out.sort_by_key(|c| c.start_row);
    out
}

fn merge_adjacent_chunks(chunks: Vec<Chunk>) -> Vec<Chunk> {
    if chunks.len() <= 1 {
        return chunks;
    }
    let mut merged: Vec<Chunk> = Vec::new();
    let mut parts = vec![chunks[0].content.clone()];
    let mut start_row = chunks[0].start_row;
    let mut end_row = chunks[0].end_row;
    for current in &chunks[1..] {
        if end_row + 1 == current.start_row {
            parts.push(current.content.clone());
            end_row = current.end_row;
        } else {
            merged.push(Chunk {
                content: parts.join("\n"),
                start_row,
                end_row,
            });
            parts = vec![current.content.clone()];
            start_row = current.start_row;
            end_row = current.end_row;
        }
    }
    merged.push(Chunk {
        content: parts.join("\n"),
        start_row,
        end_row,
    });
    merged
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guesses_language() {
        assert_eq!(Language::guess("src/main.rs"), Some(Language::Rust));
        assert_eq!(Language::guess("a/b.ts"), Some(Language::TypeScript));
        assert_eq!(Language::guess("a/b.txt"), None);
    }

    #[test]
    fn compresses_rust() {
        let code = "use std::fs;\n\n/// Doc\nfn main() {\n    println!(\"hi\");\n}\n";
        let out = compress_with_language(Language::Rust, code).unwrap();
        assert!(out.contains("use std::fs;"));
        assert!(!out.contains("println!"));
    }

    #[test]
    fn compresses_typescript() {
        let code = "export function greet(name: string): void {\n  console.log(name);\n}\n";
        let out = compress_with_language(Language::TypeScript, code).unwrap();
        assert!(out.contains("export function greet(name: string): void"));
        assert!(!out.contains("console.log"));
    }

    #[test]
    fn compresses_python() {
        let code = "@decorator\ndef greet(name):\n    print(name)\n";
        let out = compress_with_language(Language::Python, code).unwrap();
        assert!(out.contains("@decorator"));
        assert!(out.contains("def greet(name)"));
        assert!(!out.contains("print(name)"));
    }
}
