use model::Token;
use std::collections::HashSet;

/// Core parser struct that maintains parsing state
pub(crate) struct Parser<'a> {
    pub(crate) tokens: &'a [Token],
    pub(crate) pos: usize,
    pub(crate) typedefs: HashSet<String>,
}

impl<'a> Parser<'a> {
    pub fn new(tokens: &'a [Token]) -> Self {
        let mut typedefs = HashSet::new();
        typedefs.insert("__builtin_va_list".to_string());
        
        Parser {
            tokens,
            pos: 0,
            typedefs,
        }
    }

    // Token navigation utilities
    pub(crate) fn is_at_end(&self) -> bool {
        self.pos >= self.tokens.len()
    }

    pub(crate) fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    pub(crate) fn previous(&self) -> Option<&Token> {
        if self.pos == 0 {
            None
        } else {
            self.tokens.get(self.pos - 1)
        }
    }

    pub(crate) fn advance(&mut self) -> Option<&Token> {
        if !self.is_at_end() {
            self.pos += 1;
        }
        self.previous()
    }

    pub(crate) fn match_token<F>(&mut self, predicate: F) -> bool
    where
        F: Fn(&Token) -> bool,
    {
        if self.check(&predicate) {
            self.advance();
            true
        } else {
            false
        }
    }

    pub(crate) fn check<F>(&self, predicate: &F) -> bool
    where
        F: Fn(&Token) -> bool,
    {
        match self.peek() {
            Some(tok) => predicate(tok),
            None => false,
        }
    }

    pub(crate) fn check_at<F>(&self, offset: usize, predicate: F) -> bool
    where
        F: Fn(&Token) -> bool,
    {
        self.tokens.get(self.pos + offset).map_or(false, predicate)
    }

    pub(crate) fn expect<F>(&mut self, predicate: F, expected: &str) -> Result<(), String>
    where
        F: Fn(&Token) -> bool,
    {
        if self.check(&predicate) {
            self.advance();
            Ok(())
        } else {
            let peeked = self.peek();
            // eprintln!("Parse error at pos {}: expected {}, found {:?}", self.pos, expected, peeked);
            Err(format!("expected {expected}, found {:?} at position {}", peeked, self.pos))
        }
    }
}
