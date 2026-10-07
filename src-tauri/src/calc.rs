//! Arithmetic for the search box: `2^10`, `(1+2)*3`, `10 % 3`, `sqrt 2`,
//! `sin(30deg)`, `5!`, `2pi`, and `$1 + $2` where `$N` is the N-th most
//! recent clipboard history entry parsed as a number.
//!
//! A query only counts as a calculation when it contains an operator or a
//! function call, so plain searches such as `e` or `2024` are left alone.

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Num(f64),
    Ident(String),
    Op(char),
    LParen,
    RParen,
}

fn normalize(c: char) -> char {
    match c {
        '×' | '＊' | '·' => '*',
        '÷' | '／' => '/',
        '（' => '(',
        '）' => ')',
        '＋' => '+',
        '－' | '−' => '-',
        '％' => '%',
        '＾' => '^',
        '！' => '!',
        '。' => '.',
        '，' => ',',
        c if ('０'..='９').contains(&c) => char::from_u32(c as u32 - '０' as u32 + '0' as u32).unwrap_or(c),
        c => c,
    }
}

fn tokenize(input: &str) -> Option<Vec<Token>> {
    let chars: Vec<char> = input.chars().map(normalize).collect();
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() || c == '_' {
            i += 1;
        } else if c.is_ascii_digit() || (c == '.' && chars.get(i + 1).is_some_and(|d| d.is_ascii_digit())) {
            let start = i;
            while i < chars.len()
                && (chars[i].is_ascii_digit()
                    || chars[i] == '.'
                    || chars[i] == '_'
                    || (chars[i] == ',' && chars.get(i + 1).is_some_and(|d| d.is_ascii_digit())))
            {
                i += 1;
            }
            let mantissa: String = chars[start..i].iter().collect();
            if !valid_grouping(&mantissa) {
                return None;
            }
            // Scientific notation: 1e3, 2.5e-4 (but not `2e` = 2*e).
            if i < chars.len() && (chars[i] == 'e' || chars[i] == 'E') {
                let mut j = i + 1;
                if j < chars.len() && (chars[j] == '+' || chars[j] == '-') {
                    j += 1;
                }
                if j < chars.len() && chars[j].is_ascii_digit() {
                    while j < chars.len() && chars[j].is_ascii_digit() {
                        j += 1;
                    }
                    i = j;
                }
            }
            let text: String = chars[start..i]
                .iter()
                .filter(|c| **c != '_' && **c != ',')
                .collect();
            tokens.push(Token::Num(text.parse().ok()?));
        } else if c.is_alphabetic() || c == 'π' {
            let start = i;
            while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == 'π') {
                i += 1;
            }
            tokens.push(Token::Ident(chars[start..i].iter().collect::<String>().to_lowercase()));
        } else {
            tokens.push(match c {
                '(' => Token::LParen,
                ')' => Token::RParen,
                '+' | '-' | '*' | '/' | '^' | '%' | '!' => Token::Op(c),
                _ => return None,
            });
            i += 1;
        }
    }
    Some(tokens)
}

/// Thousands separators are only accepted in their standard places: the
/// integer part split into a leading group of 1–3 digits followed by groups
/// of exactly 3 (`1,234,567.89`). Anything else, e.g. `1,23` or `1.2,345`,
/// is rejected rather than silently read as a different number.
fn valid_grouping(mantissa: &str) -> bool {
    if !mantissa.contains(',') {
        return true;
    }
    let (int, frac) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    if frac.contains(',') {
        return false;
    }
    let mut groups = int.split(',');
    let first_ok = groups
        .next()
        .is_some_and(|g| (1..=3).contains(&g.len()) && g.bytes().all(|b| b.is_ascii_digit()));
    first_ok && groups.all(|g| g.len() == 3 && g.bytes().all(|b| b.is_ascii_digit()))
}

fn constant(name: &str) -> Option<f64> {
    Some(match name {
        "pi" | "π" => std::f64::consts::PI,
        "e" => std::f64::consts::E,
        "tau" => std::f64::consts::TAU,
        "deg" => std::f64::consts::PI / 180.0,
        _ => return None,
    })
}

fn function(name: &str) -> Option<fn(f64) -> f64> {
    Some(match name {
        "sqrt" => f64::sqrt,
        "cbrt" => f64::cbrt,
        "abs" => f64::abs,
        "sin" => f64::sin,
        "cos" => f64::cos,
        "tan" => f64::tan,
        "asin" => f64::asin,
        "acos" => f64::acos,
        "atan" => f64::atan,
        "ln" => f64::ln,
        "log" => f64::log10,
        "log2" => f64::log2,
        "exp" => f64::exp,
        "floor" => f64::floor,
        "ceil" => f64::ceil,
        "round" => f64::round,
        _ => return None,
    })
}

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    /// Set once a binary/postfix operator or a function is seen.
    is_calculation: bool,
}

impl Parser {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    fn next(&mut self) -> Option<Token> {
        let t = self.tokens.get(self.pos).cloned();
        self.pos += 1;
        t
    }

    fn eat_op(&mut self, ops: &[char]) -> Option<char> {
        match self.peek() {
            Some(Token::Op(c)) if ops.contains(c) => {
                let c = *c;
                self.pos += 1;
                Some(c)
            }
            _ => None,
        }
    }

    // expr := term (('+' | '-') term)*
    fn expr(&mut self) -> Option<f64> {
        let mut value = self.term()?;
        while let Some(op) = self.eat_op(&['+', '-']) {
            self.is_calculation = true;
            let rhs = self.term()?;
            value = if op == '+' { value + rhs } else { value - rhs };
        }
        Some(value)
    }

    // term := unary (('*' | '/' | '%') unary | implicit-multiplication)*
    fn term(&mut self) -> Option<f64> {
        let mut value = self.unary()?;
        loop {
            if let Some(op) = self.eat_op(&['*', '/', '%']) {
                self.is_calculation = true;
                let rhs = self.unary()?;
                value = match op {
                    '*' => value * rhs,
                    '/' => value / rhs,
                    // Mathematical modulo: the result takes the divisor's sign
                    // for positive divisors, e.g. -7 % 3 = 2.
                    _ => value.rem_euclid(rhs),
                };
            } else if matches!(self.peek(), Some(Token::LParen | Token::Ident(_))) {
                // `2pi`, `3(4+5)`, `30deg`
                self.is_calculation = true;
                value *= self.unary()?;
            } else {
                return Some(value);
            }
        }
    }

    // unary := ('-' | '+') unary | power
    fn unary(&mut self) -> Option<f64> {
        match self.eat_op(&['-', '+']) {
            Some('-') => Some(-self.unary()?),
            Some(_) => self.unary(),
            None => self.power(),
        }
    }

    // power := postfix ('^' unary)?   (right associative)
    fn power(&mut self) -> Option<f64> {
        let base = self.postfix()?;
        if self.eat_op(&['^']).is_some() {
            self.is_calculation = true;
            return Some(base.powf(self.unary()?));
        }
        Some(base)
    }

    // postfix := primary '!'*
    fn postfix(&mut self) -> Option<f64> {
        let mut value = self.primary()?;
        while self.eat_op(&['!']).is_some() {
            self.is_calculation = true;
            value = factorial(value)?;
        }
        Some(value)
    }

    fn primary(&mut self) -> Option<f64> {
        match self.next()? {
            Token::Num(n) => Some(n),
            Token::LParen => {
                let value = self.expr()?;
                // A missing closing parenthesis at the end is tolerated.
                match self.next() {
                    Some(Token::RParen) | None => Some(value),
                    _ => None,
                }
            }
            Token::Ident(name) => {
                if let Some(f) = function(&name) {
                    self.is_calculation = true;
                    let arg = if self.peek() == Some(&Token::LParen) {
                        self.primary()?
                    } else {
                        self.unary()?
                    };
                    Some(f(arg))
                } else {
                    constant(&name)
                }
            }
            _ => None,
        }
    }
}

fn factorial(n: f64) -> Option<f64> {
    if n < 0.0 || n.fract() != 0.0 || n > 170.0 {
        return None;
    }
    Some((1..=n as u64).fold(1.0, |acc, k| acc * k as f64))
}

pub struct Calculation {
    /// Result formatted for pasting.
    pub result: String,
    /// The input with `$N` references replaced by their values.
    pub expression: String,
}

/// Evaluates `input` if it looks like a calculation. `clip(n)` returns the
/// n-th most recent clipboard entry (1 = latest) for `$n` references; an
/// unknown or non-numeric reference makes the whole input not a calculation.
pub fn calculate(input: &str, clip: impl Fn(usize) -> Option<String>) -> Option<Calculation> {
    let (eval_src, expression) = substitute_clips(input.trim(), clip)?;
    let value = evaluate(&eval_src)?;
    Some(Calculation {
        result: format(value),
        expression,
    })
}

/// Replaces `$N` with the numeric value of that clipboard entry. Returns the
/// text to evaluate (values parenthesized so `2$1` and `-$1` stay correct)
/// and the text to display.
fn substitute_clips(input: &str, clip: impl Fn(usize) -> Option<String>) -> Option<(String, String)> {
    if !input.contains(['$', '＄']) {
        return Some((input.to_string(), input.to_string()));
    }
    let mut eval_src = String::new();
    let mut display = String::new();
    let mut chars = input.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '$' && c != '＄' {
            eval_src.push(c);
            display.push(c);
            continue;
        }
        let mut digits = String::new();
        while let Some(d) = chars.peek().map(|d| normalize(*d)).filter(char::is_ascii_digit) {
            digits.push(d);
            chars.next();
        }
        let n: usize = digits.parse().ok().filter(|n| *n > 0)?;
        let value = format(parse_number(&clip(n)?)?);
        eval_src.push_str(&format!("({value})"));
        let glued = display
            .chars()
            .next_back()
            .is_some_and(|p| p.is_alphanumeric() || p == ')');
        if value.starts_with('-') || glued {
            display.push_str(&format!("({value})"));
        } else {
            display.push_str(&value);
        }
    }
    Some((eval_src, display))
}

/// Reads copied text as a number, tolerating thousands separators,
/// surrounding whitespace, currency symbols, full-width digits and a
/// trailing percent sign (`15%` = 0.15).
fn parse_number(text: &str) -> Option<f64> {
    let mut cleaned: String = text
        .trim()
        .chars()
        .map(normalize)
        .filter(|c| !matches!(c, ',' | '，' | '_' | '$' | '¥' | '￥' | '€' | '£') && !c.is_whitespace())
        .collect();
    let percent = cleaned.ends_with('%');
    if percent {
        cleaned.pop();
    }
    if !cleaned.chars().any(|c| c.is_ascii_digit())
        || !cleaned.chars().all(|c| c.is_ascii_digit() || matches!(c, '.' | '-' | '+' | 'e' | 'E'))
    {
        return None;
    }
    let value: f64 = cleaned.parse().ok()?;
    let value = if percent { value / 100.0 } else { value };
    value.is_finite().then_some(value)
}

fn evaluate(input: &str) -> Option<f64> {
    let tokens = tokenize(input.trim().trim_end_matches('='))?;
    if tokens.is_empty() {
        return None;
    }
    let mut parser = Parser {
        tokens,
        pos: 0,
        is_calculation: false,
    };
    let value = parser.expr()?;
    let complete = parser.pos >= parser.tokens.len();
    (complete && parser.is_calculation && value.is_finite()).then_some(value)
}

/// Plain representation used for pasting: no grouping, at most 10 decimals,
/// scientific notation for very large or small magnitudes.
pub fn format(value: f64) -> String {
    let value = if value == 0.0 { 0.0 } else { value }; // drop -0
    let abs = value.abs();
    if abs != 0.0 && !(1e-9..1e15).contains(&abs) {
        let s = format!("{value:e}");
        return match s.split_once('e') {
            Some((mantissa, exp)) => {
                let mantissa: f64 = mantissa.parse().unwrap_or(0.0);
                format!("{}e{}", trim_decimals(&format!("{mantissa:.10}")), exp)
            }
            None => s,
        };
    }
    // f64 carries ~15–16 significant digits; printing beyond that exposes
    // binary rounding noise (12345678.9 * 2 = 24691357.800000001).
    let int_digits = if abs < 1.0 { 0 } else { abs.log10().floor() as i32 + 1 };
    let decimals = (15 - int_digits).clamp(0, 10) as usize;
    trim_decimals(&format!("{value:.decimals$}"))
}

fn trim_decimals(s: &str) -> String {
    if s.contains('.') {
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn calc(s: &str) -> Option<String> {
        calculate(s, |_| None).map(|c| c.result)
    }

    #[test]
    fn arithmetic() {
        assert_eq!(calc("1+2*3").as_deref(), Some("7"));
        assert_eq!(calc("(1+2)*3").as_deref(), Some("9"));
        assert_eq!(calc("2^3^2").as_deref(), Some("512"));
        assert_eq!(calc("-2^2").as_deref(), Some("-4"));
        assert_eq!(calc("0.1+0.2").as_deref(), Some("0.3"));
        assert_eq!(calc("10/4").as_deref(), Some("2.5"));
        assert_eq!(calc("1_000*3").as_deref(), Some("3000"));
        assert_eq!(calc("1e3+1").as_deref(), Some("1001"));
        assert_eq!(calc("(1+2").as_deref(), Some("3"));
        assert_eq!(calc("3*4=").as_deref(), Some("12"));
    }

    #[test]
    fn modulo() {
        assert_eq!(calc("10 % 3").as_deref(), Some("1"));
        assert_eq!(calc("-7%3").as_deref(), Some("2"));
        assert_eq!(calc("7.5%2").as_deref(), Some("1.5"));
        assert_eq!(calc("2*7%4").as_deref(), Some("2"));
        assert_eq!(calc("5%0"), None);
        assert_eq!(calc("200*15%"), None);
    }

    #[test]
    fn clipboard_references() {
        // Copied "12" first, then "1,000.5": $1 is the latest.
        let history = ["1,000.5", " 12\n", "hello", "-3", "15%"];
        let clip = |n: usize| history.get(n - 1).map(|s| s.to_string());
        let run = |q: &str| calculate(q, clip).map(|c| (c.result, c.expression));

        assert_eq!(run("$1+$2"), Some(("1012.5".into(), "1000.5+12".into())));
        assert_eq!(run("$2 * $4"), Some(("-36".into(), "12 * (-3)".into())));
        assert_eq!(run("2$2"), Some(("24".into(), "2(12)".into())));
        assert_eq!(run("-$2^2"), Some(("-144".into(), "-12^2".into())));
        assert_eq!(run("$2*$5"), Some(("1.8".into(), "12*0.15".into())));
        assert_eq!(run("$1"), None, "a lone reference is not a calculation");
        assert_eq!(run("$3+1"), None, "non-numeric clip");
        assert_eq!(run("$9+1"), None, "missing clip");
        assert_eq!(run("$0+1"), None);
        assert_eq!(run("$+1"), None);
    }

    #[test]
    fn functions_and_factorial() {
        assert_eq!(calc("5!").as_deref(), Some("120"));
        assert_eq!(calc("sqrt 16").as_deref(), Some("4"));
        assert_eq!(calc("sqrt(16)+1").as_deref(), Some("5"));
        assert_eq!(calc("sin(30deg)").as_deref(), Some("0.5"));
        assert_eq!(calc("2pi").as_deref(), Some("6.2831853072"));
        assert_eq!(calc("log(1000)").as_deref(), Some("3"));
    }

    #[test]
    fn full_width_input() {
        assert_eq!(calc("（１＋２）×３÷２").as_deref(), Some("4.5"));
    }

    #[test]
    fn thousands_separators() {
        assert_eq!(calc("1,234+5").as_deref(), Some("1239"));
        assert_eq!(calc("12,345,678.9*2").as_deref(), Some("24691357.8"));
        assert_eq!(calc("1,000 / 8").as_deref(), Some("125"));
        assert_eq!(calc("-1,500+500").as_deref(), Some("-1000"));
        assert_eq!(calc("2*1,000e3").as_deref(), Some("2000000"));
        assert_eq!(calc("１，２３４＋１").as_deref(), Some("1235"));
        assert_eq!(calc("sqrt(1,000,000)").as_deref(), Some("1000"));
    }

    #[test]
    fn rejects_misplaced_separators() {
        for q in ["1,23+1", "1234,567+1", "1,2345+1", "1.234,5+1", "1,,234+1", "1,+2", ",5+1", "1_000,000+1"] {
            assert_eq!(calc(q), None, "{q}");
        }
    }

    #[test]
    fn ignores_plain_searches() {
        for q in ["", "e", "pi", "2024", "-5", "hello", "git log", "a-b", "1/0", "sqrt(-1)", "(", "3 +"] {
            assert_eq!(calc(q), None, "{q}");
        }
    }

    #[test]
    fn formats_extremes() {
        assert_eq!(format(1e20), "1e20");
        assert_eq!(format(1.5e-12), "1.5e-12");
        assert_eq!(format(-0.0), "0");
        assert_eq!(format(12345678.9 * 2.0), "24691357.8");
        assert_eq!(format(0.1 * 3.0), "0.3");
        assert_eq!(format(123456789012345.6), "123456789012346");
        assert_eq!(format(1.0 / 3.0), "0.3333333333");
    }
}
