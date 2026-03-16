use std::collections::HashMap;
use crate::utils::{Helper, Config};

pub struct Parser {
    input: String,
    tokens: Vec<Token>,
}

pub enum Token {
    Word(String),
    Number(i64),
    Eof,
}

pub trait Parseable {
    fn parse(&self) -> Result<(), Error>;
    fn validate(&self) -> bool { true }
}

impl Parseable for Parser {
    fn parse(&self) -> Result<(), Error> {
        Ok(())
    }
}

impl Parser {
    pub fn new(input: String) -> Self {
        Self { input, tokens: Vec::new() }
    }

    fn tokenize(&mut self) {}
}

pub const MAX_DEPTH: usize = 100;
static INSTANCE_COUNT: i32 = 0;
pub type ParseResult<T> = Result<T, Error>;

macro_rules! parse_assert {
    ($e:expr) => {};
}

mod internal {
    pub fn helper() {}
}
