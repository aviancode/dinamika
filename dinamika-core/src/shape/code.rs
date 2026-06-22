//! Code shape — source highlighting with a manually configured palette.
//!
//! The code shape ([`Shape::code`](crate::Shape::code)) is the same text shape
//! ([`Shape::text`](crate::Shape::text)) in everything concerning font, font
//! size, layout, content edits and animations (spawn, typing, smoothing). There
//! is exactly one difference: it has **no single color**
//! ([`color`](crate::Shape::color) panics on it) — instead glyphs are colored
//! per-character by the result of syntax parsing.
//!
//! # A palette instead of a color
//!
//! Colors are set by a [`Palette`] — a "token category → color" table, assembled
//! manually with a fluent builder. The base color ([`Palette::new`]) is given to
//! all characters for which the palette has no rule; named categories
//! ([`keyword`](Palette::keyword), [`string`](Palette::string),
//! [`comment`](Palette::comment), etc.) override it for their tokens. An empty
//! palette (without a single category) colors all the code with the base color —
//! like plain text, until highlighting is configured.
//!
//! # Choosing the language
//!
//! The grammar is set by [`Language`] (for example [`Rust`](Language::Rust) or
//! [`JavaScript`](Language::JavaScript)). The default is
//! [`PlainText`](Language::PlainText): no parsing, all the code in the base
//! color. Parsing relies on the built-in Sublime Text grammars from `syntect`.
//!
//! ```no_run
//! use dinamika_core::*;
//!
//! let bytes = std::fs::read("Consolas.ttf").unwrap();
//! let palette = Palette::new(Color::from_rgba8(212, 212, 212, 255))
//!     .keyword(Color::from_rgba8(197, 134, 192, 255))
//!     .string(Color::from_rgba8(206, 145, 120, 255))
//!     .comment(Color::from_rgba8(106, 153, 85, 255))
//!     .number(Color::from_rgba8(181, 206, 168, 255))
//!     .function(Color::from_rgba8(220, 220, 170, 255));
//!
//! let snippet = Shape::code("fn main() {\n    println!(\"hi\");\n}")
//!     .font(bytes)
//!     .font_size(28.0)
//!     .language(Language::Rust)
//!     .palette(palette);
//! ```
//!
//! # Parsing and cache
//!
//! The palette does not depend on `syntect`: parsing only splits the source into
//! scopes (`keyword.control`, `string.quoted`, `comment.line`, …), and mapping a
//! scope to a color is done by the [`Palette`] (see [`classify`]). The result —
//! a color per character of the string — is cached by a key (text, language,
//! palette), so static code is not re-parsed every frame. The glyph layout itself
//! and the path groups by color are assembled in [`text`](super::text) — here are
//! only the colors.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::OnceLock;

use dinamika_cpu::Color;
use syntect::parsing::{ParseState, Scope, ScopeStack, SyntaxSet};

/// The code shape's highlight language. Resolves to a Sublime Text grammar from
/// the built-in `syntect` set; an unknown grammar (or
/// [`PlainText`](Language::PlainText)) means "no parsing" — all the code is
/// colored with the palette's base color.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Default)]
pub enum Language {
    /// No highlighting (the default value): the code is colored with the base color.
    #[default]
    PlainText,
    /// Rust.
    Rust,
    /// JavaScript.
    JavaScript,
    /// Python.
    Python,
    /// C.
    C,
    /// C++.
    Cpp,
    /// Go.
    Go,
    /// JSON.
    Json,
    /// HTML.
    Html,
    /// CSS.
    Css,
    /// Java.
    Java,
    /// Bash / shell.
    Bash,
}

impl Language {
    /// The `syntect` grammar token (name/extension). `None` —
    /// [`PlainText`](Language::PlainText), no parsing.
    fn token(self) -> Option<&'static str> {
        Some(match self {
            Language::PlainText => return None,
            Language::Rust => "rust",
            Language::JavaScript => "js",
            Language::Python => "python",
            Language::C => "c",
            Language::Cpp => "c++",
            Language::Go => "go",
            Language::Json => "json",
            Language::Html => "html",
            Language::Css => "css",
            Language::Java => "java",
            Language::Bash => "sh",
        })
    }
}

/// A highlight palette: "token category → color", assembled manually.
///
/// The base color ([`new`](Palette::new)) is the default color for all
/// characters; named categories override it for their tokens. An unset category
/// (`None`) falls back to the base color, so it is enough to set only the colors
/// you need. The palette is cheap and comparable by value — it can be cloned and
/// passed to [`Shape::palette`](crate::Shape::palette).
#[derive(Clone, Debug, PartialEq)]
pub struct Palette {
    /// The base color (for tokens without a rule in the palette).
    foreground: Color,
    /// Keywords (`if`, `fn`, `let`, `class`, modifiers…).
    keyword: Option<Color>,
    /// String literals.
    string: Option<Color>,
    /// Comments.
    comment: Option<Color>,
    /// Numeric literals.
    number: Option<Color>,
    /// Function names (declarations and calls).
    function: Option<Color>,
    /// Names of types, classes, structs.
    type_: Option<Color>,
    /// Language and named constants (`true`, `null`, `NaN`…).
    constant: Option<Color>,
    /// Operators (`=`, `+`, `&&`…).
    operator: Option<Color>,
    /// Variable names.
    variable: Option<Color>,
    /// Punctuation (brackets, commas, semicolons).
    punctuation: Option<Color>,
}

impl Default for Palette {
    /// An empty palette: base color [`Color::BLACK`], without a single category —
    /// the code is colored entirely with the base color until highlighting is
    /// configured.
    fn default() -> Self {
        Palette::new(Color::BLACK)
    }
}

impl Palette {
    /// A palette with base color `foreground` and no highlight rules. Categories
    /// are added with the fluent methods ([`keyword`](Palette::keyword), etc.).
    pub fn new(foreground: Color) -> Self {
        Palette {
            foreground,
            keyword: None,
            string: None,
            comment: None,
            number: None,
            function: None,
            type_: None,
            constant: None,
            operator: None,
            variable: None,
            punctuation: None,
        }
    }

    /// Keyword color.
    pub fn keyword(mut self, c: Color) -> Self {
        self.keyword = Some(c);
        self
    }
    /// String-literal color.
    pub fn string(mut self, c: Color) -> Self {
        self.string = Some(c);
        self
    }
    /// Comment color.
    pub fn comment(mut self, c: Color) -> Self {
        self.comment = Some(c);
        self
    }
    /// Numeric-literal color.
    pub fn number(mut self, c: Color) -> Self {
        self.number = Some(c);
        self
    }
    /// Function-name color.
    pub fn function(mut self, c: Color) -> Self {
        self.function = Some(c);
        self
    }
    /// Type/class-name color.
    pub fn type_(mut self, c: Color) -> Self {
        self.type_ = Some(c);
        self
    }
    /// Constant color.
    pub fn constant(mut self, c: Color) -> Self {
        self.constant = Some(c);
        self
    }
    /// Operator color.
    pub fn operator(mut self, c: Color) -> Self {
        self.operator = Some(c);
        self
    }
    /// Variable-name color.
    pub fn variable(mut self, c: Color) -> Self {
        self.variable = Some(c);
        self
    }
    /// Punctuation color.
    pub fn punctuation(mut self, c: Color) -> Self {
        self.punctuation = Some(c);
        self
    }

    /// A palette without a single highlight rule — all the code will go in the
    /// base color, so syntax parsing can be skipped.
    fn is_plain(&self) -> bool {
        self.keyword.is_none()
            && self.string.is_none()
            && self.comment.is_none()
            && self.number.is_none()
            && self.function.is_none()
            && self.type_.is_none()
            && self.constant.is_none()
            && self.operator.is_none()
            && self.variable.is_none()
            && self.punctuation.is_none()
    }

    /// The token's color by its scope stack (outer to inner, as `syntect`
    /// returns it).
    ///
    /// Comments and strings color their whole region (any `comment*`/`string*`
    /// scope in the stack decides the outcome), otherwise the color is taken from
    /// the most specific (innermost) scope, falling back outward: the first scope
    /// whose category is set in the palette gives the color. If nothing matched —
    /// the base color.
    fn color_for(&self, scopes: &[Scope]) -> Color {
        for scope in scopes {
            let s = scope.build_string();
            if s.starts_with("comment") {
                return self.comment.unwrap_or(self.foreground);
            }
            if s.starts_with("string") {
                return self.string.unwrap_or(self.foreground);
            }
        }
        for scope in scopes.iter().rev() {
            if let Some(color) = classify(&scope.build_string()).and_then(|c| self.category_color(c)) {
                return color;
            }
        }
        self.foreground
    }

    /// The color of a specific category (if set in the palette).
    fn category_color(&self, category: Category) -> Option<Color> {
        match category {
            Category::Keyword => self.keyword,
            Category::Number => self.number,
            Category::Function => self.function,
            Category::Type => self.type_,
            Category::Constant => self.constant,
            Category::Operator => self.operator,
            Category::Variable => self.variable,
            Category::Punctuation => self.punctuation,
        }
    }
}

/// The token category a `syntect` scope maps to. Comments and strings are handled
/// separately ([`Palette::color_for`]) and do not get here.
#[derive(Copy, Clone)]
enum Category {
    Keyword,
    Number,
    Function,
    Type,
    Constant,
    Operator,
    Variable,
    Punctuation,
}

/// Maps a Sublime Text scope string (`keyword.control.rust`,
/// `entity.name.function`, …) to a palette category. `None` — a scope without a
/// category (its color is looked up in a more outer scope or taken as the base).
fn classify(scope: &str) -> Option<Category> {
    if scope.starts_with("keyword.operator") {
        Some(Category::Operator)
    } else if scope.starts_with("keyword") || scope.starts_with("storage") {
        // storage.type / storage.modifier are `fn`, `let`, `const`, `pub`… —
        // keywords in meaning.
        Some(Category::Keyword)
    } else if scope.starts_with("constant.numeric") {
        Some(Category::Number)
    } else if scope.starts_with("constant") {
        Some(Category::Constant)
    } else if scope.starts_with("entity.name.function")
        || scope.starts_with("support.function")
        || scope.starts_with("variable.function")
        || scope.starts_with("meta.function-call")
    {
        Some(Category::Function)
    } else if scope.starts_with("entity.name.type")
        || scope.starts_with("entity.name.class")
        || scope.starts_with("entity.name.struct")
        || scope.starts_with("entity.name.enum")
        || scope.starts_with("entity.name.trait")
        || scope.starts_with("entity.name.namespace")
        || scope.starts_with("support.type")
        || scope.starts_with("support.class")
        || scope.starts_with("entity.other.inherited-class")
    {
        Some(Category::Type)
    } else if scope.starts_with("variable") {
        Some(Category::Variable)
    } else if scope.starts_with("punctuation") {
        Some(Category::Punctuation)
    } else {
        None
    }
}

/// The code shape's highlight state: palette, language and a cache of computed
/// colors.
///
/// Lives next to [`TextData`](super::text::TextData) inside
/// [`ShapeData`](super::ShapeData) for shapes of kind
/// [`ShapeKind::Code`](super::ShapeKind::Code). The text part (content, font,
/// style, animations) is stored in `TextData`; here is only what replaces the
/// color.
pub(crate) struct CodeData {
    palette: RefCell<Palette>,
    language: Cell<Language>,
    /// A small "color per character" cache keyed by (text, language, palette).
    /// The size is enough for static code and for both ends (`from`/`to`) of the
    /// smoothing morph, which are computed every frame.
    cache: RefCell<Vec<(CodeKey, Rc<Vec<Color>>)>>,
}

/// The color-cache key: everything the parsing and coloring result depends on.
#[derive(Clone, PartialEq)]
struct CodeKey {
    text: String,
    language: Language,
    palette: Palette,
}

/// How many recent parses to keep in the cache.
const CACHE_CAP: usize = 4;

impl CodeData {
    /// Default code: an empty palette (base black) and [`Language::PlainText`] —
    /// no highlighting until it is configured.
    pub(crate) fn new() -> Self {
        CodeData {
            palette: RefCell::new(Palette::default()),
            language: Cell::new(Language::default()),
            cache: RefCell::new(Vec::new()),
        }
    }

    /// Sets the palette and clears the color cache.
    pub(crate) fn set_palette(&self, palette: Palette) {
        *self.palette.borrow_mut() = palette;
        self.cache.borrow_mut().clear();
    }

    /// Sets the highlight language and clears the color cache.
    pub(crate) fn set_language(&self, language: Language) {
        self.language.set(language);
        self.cache.borrow_mut().clear();
    }

    /// The palette's base color — the color of characters without a highlight
    /// rule.
    pub(crate) fn foreground(&self) -> Color {
        self.palette.borrow().foreground
    }

    /// The color of each character of the string `text` (aligned to
    /// `text.chars()`: one color per Unicode scalar, including `\n`). The result
    /// is cached by (text, language, palette).
    pub(crate) fn char_colors(&self, text: &str) -> Rc<Vec<Color>> {
        let palette = self.palette.borrow().clone();
        let language = self.language.get();
        if let Some(hit) = self.cache.borrow().iter().find_map(|(k, v)| {
            (k.text == text && k.language == language && k.palette == palette).then(|| Rc::clone(v))
        }) {
            return hit;
        }
        let colors = Rc::new(compute_colors(text, language, &palette));
        let mut cache = self.cache.borrow_mut();
        cache.insert(0, (CodeKey { text: text.to_owned(), language, palette }, Rc::clone(&colors)));
        cache.truncate(CACHE_CAP);
        colors
    }
}

/// The built-in `syntect` grammar set (loaded once per process).
fn syntax_set() -> &'static SyntaxSet {
    static SET: OnceLock<SyntaxSet> = OnceLock::new();
    SET.get_or_init(SyntaxSet::load_defaults_newlines)
}

/// Computes the color of each character of `text`: parses the source into scopes
/// with the grammar `language` and colors each character via
/// [`Palette::color_for`]. Returns one color per character (aligned to
/// `text.chars()`).
///
/// Without a language, with an empty palette or for a grammar that isn't found,
/// parsing is skipped — all characters get the base color.
fn compute_colors(text: &str, language: Language, palette: &Palette) -> Vec<Color> {
    let count = text.chars().count();
    let foreground = palette.foreground;
    let token = match language.token() {
        Some(token) if !palette.is_plain() => token,
        _ => return vec![foreground; count],
    };
    let syntax_set = syntax_set();
    let syntax = match syntax_set.find_syntax_by_token(token) {
        Some(syntax) => syntax,
        None => return vec![foreground; count],
    };

    let mut parse = ParseState::new(syntax);
    let mut stack = ScopeStack::new();
    let mut colors = Vec::with_capacity(count);
    // Lines with `\n` kept: the newlines-set grammars and the parser state
    // between lines are designed exactly for this (block comments, etc.).
    for line in text.split_inclusive('\n') {
        let ops = parse.parse_line(line, syntax_set).unwrap_or_default();
        let mut ops = ops.into_iter().peekable();
        for (byte_offset, _ch) in line.char_indices() {
            // Apply all scope changes starting no later than this character.
            while let Some(&(offset, _)) = ops.peek() {
                if offset <= byte_offset {
                    let (_, op) = ops.next().unwrap();
                    let _ = stack.apply(&op);
                } else {
                    break;
                }
            }
            colors.push(palette.color_for(stack.as_slice()));
        }
        // Trailing scope changes after the last character of the line.
        for (_, op) in ops {
            let _ = stack.apply(&op);
        }
    }
    colors
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The index of the first character of the substring `needle` in `text` (in
    /// scalar indices).
    fn char_index_of(text: &str, needle: &str) -> usize {
        let byte = text.find(needle).expect("the substring is present in the text");
        text[..byte].chars().count()
    }

    #[test]
    fn plain_language_paints_everything_with_foreground() {
        // Without a highlight language all the code is in the base color, even with a palette set.
        let fg = Color::from_rgba8(10, 20, 30, 255);
        let palette = Palette::new(fg).keyword(Color::from_rgba8(200, 0, 0, 255));
        let code = CodeData::new();
        code.set_palette(palette);
        let text = "fn main() {}";
        let colors = code.char_colors(text);
        assert_eq!(colors.len(), text.chars().count());
        assert!(colors.iter().all(|c| *c == fg), "only the base color was expected");
    }

    #[test]
    fn empty_palette_skips_highlighting() {
        // A language is set, but the palette is empty — no highlighting, all in the base color.
        let fg = Color::from_rgba8(1, 2, 3, 255);
        let code = CodeData::new();
        code.set_palette(Palette::new(fg));
        code.set_language(Language::Rust);
        let colors = code.char_colors("fn main() {}");
        assert!(colors.iter().all(|c| *c == fg));
    }

    #[test]
    fn rust_keyword_string_and_comment_take_palette_colors() {
        let fg = Color::from_rgba8(212, 212, 212, 255);
        let kw = Color::from_rgba8(197, 134, 192, 255);
        let st = Color::from_rgba8(206, 145, 120, 255);
        let cm = Color::from_rgba8(106, 153, 85, 255);
        let palette = Palette::new(fg).keyword(kw).string(st).comment(cm);
        let code = CodeData::new();
        code.set_palette(palette);
        code.set_language(Language::Rust);

        let text = "let x = \"hi\"; // note";
        let colors = code.char_colors(text);
        assert_eq!(colors.len(), text.chars().count());

        // `let` is a keyword.
        assert_eq!(colors[char_index_of(text, "let")], kw);
        // A character inside a string literal — the string color.
        assert_eq!(colors[char_index_of(text, "hi")], st);
        // Inside a comment — the comment color (including `//`).
        assert_eq!(colors[char_index_of(text, "// note")], cm);
        assert_eq!(colors[char_index_of(text, "note")], cm);
    }

    #[test]
    fn unset_category_falls_back_to_foreground() {
        // A category without a rule (here — number) falls back to the base color.
        let fg = Color::from_rgba8(50, 50, 50, 255);
        let kw = Color::from_rgba8(0, 100, 200, 255);
        let palette = Palette::new(fg).keyword(kw); // number is not set
        let code = CodeData::new();
        code.set_palette(palette);
        code.set_language(Language::Rust);

        let text = "let n = 42;";
        let colors = code.char_colors(text);
        assert_eq!(colors[char_index_of(text, "let")], kw);
        assert_eq!(colors[char_index_of(text, "42")], fg, "a number without a rule — in the base color");
    }

    #[test]
    fn colors_align_with_multiline_chars() {
        // The result length matches the number of scalars, including `\n`.
        let code = CodeData::new();
        code.set_language(Language::Rust);
        code.set_palette(Palette::new(Color::BLACK).keyword(Color::WHITE));
        let text = "fn a() {}\nfn b() {}\n";
        let colors = code.char_colors(text);
        assert_eq!(colors.len(), text.chars().count());
    }

    #[test]
    fn cache_returns_same_allocation_for_repeated_calls() {
        let code = CodeData::new();
        code.set_language(Language::Rust);
        code.set_palette(Palette::new(Color::BLACK).keyword(Color::WHITE));
        let a = code.char_colors("fn main() {}");
        let b = code.char_colors("fn main() {}");
        assert!(Rc::ptr_eq(&a, &b), "re-parsing the same text is taken from the cache");
        // Changing the palette clears the cache.
        code.set_palette(Palette::new(Color::BLACK).keyword(Color::from_rgba8(1, 1, 1, 255)));
        let c = code.char_colors("fn main() {}");
        assert!(!Rc::ptr_eq(&a, &c), "after changing the palette — recompute");
    }
}
