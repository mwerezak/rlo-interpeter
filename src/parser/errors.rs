use std::fmt;
use std::error::Error;
use crate::utils;
use crate::lexer::{Span, TokenMeta, LexerError};
use crate::debug::SourceError;
use crate::debug::symbol::{DebugSymbol, TokenIndex};


pub type ParseResult<T> = Result<T, ParserError>;

#[derive(Debug)]
pub enum ErrorKind {
    LexerError,
    EndofTokenStream,
    SyntaxError(String),
}

impl<S> From<S> for ErrorKind where S: ToString {
    fn from(message: S) -> Self {
        ErrorKind::SyntaxError(message.to_string())
    }
}

// Provide information about the type of syntactic construct from which the error originated
#[derive(Debug, Clone, Copy)]
pub enum ContextTag {
    Token,  // errors retrieving the actual tokens
    TopLevel,
    Sync,
    StmtMeta,
    StmtList,
    ControlFlow,
    ExprMeta,
    BlockExpr,
    FunDefExpr,
    FunParam,
    AssignmentExpr,
    VarDeclExpr,
    BinaryOpExpr,
    UnaryOpExpr,
    PrimaryExpr,
    MemberAccess,
    IndexAccess,
    ObjectCtor,
    TupleCtor,
    Atom,
    Group,
    Label,
}

impl From<ErrorKind> for ParserError {
    fn from(kind: ErrorKind) -> Self {
        Self { 
            kind, context: None, symbol: None, cause: None,
        }
    }
}

impl From<&str> for ParserError {
    fn from(message: &str) -> Self {
        Self { 
            kind: message.into(), 
            context: None, symbol: None, cause: None,
        }
    }
}

impl From<LexerError> for ParserError {
    fn from(error: LexerError) -> Self {
        Self { 
            kind: ErrorKind::LexerError, 
            context: None,
            symbol: Some((&error.span).into()),
            cause: Some(Box::new(error)),
        }
    }
}

#[derive(Debug)]
pub struct ParserError {
    kind: ErrorKind,
    context: Option<ContextTag>,
    symbol: Option<DebugSymbol>,
    cause: Option<Box<dyn Error>>,
}

impl ParserError {
    pub fn with_context_tag(mut self, context: ContextTag) -> Self {
        self.context.replace(context); self
    }
    
    pub fn with_symbol(mut self, symbol: DebugSymbol) -> Self {
        self.symbol.replace(symbol); self
    }
    
    pub fn with_symbol_from_ctx(mut self, ctx: &ErrorContext) -> Self {
        if let Some(symbol) = ctx.frame().as_debug_symbol() {
            self.symbol.replace(symbol);
        }
        self
    }
    
    pub fn with_cause(mut self, error: impl Error + 'static) -> Self {
        self.cause.replace(Box::new(error)); self
    }
    
    // fill in fields from context if not already set
    pub fn with_error_context(mut self, context: ErrorContext) -> Self {
        if self.context.is_none() {
            self.context.replace(context.frame().context());
        }
        if self.symbol.is_none() {
            self.symbol.replace(context.take_debug_symbol());
        }
        self
    }
    
    pub fn kind(&self) -> &ErrorKind { &self.kind }
    pub fn context(&self) -> Option<&ContextTag> { self.context.as_ref() }
}


impl Error for ParserError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.cause.as_ref().map(|o| o.as_ref())
    }
}

impl SourceError for ParserError {
    fn debug_symbol(&self) -> Option<&DebugSymbol> { self.symbol.as_ref() }
}

impl fmt::Display for ParserError {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        
        let message = match self.kind() {
            ErrorKind::LexerError => "",
            ErrorKind::EndofTokenStream => "unexpected end of token stream",
            ErrorKind::SyntaxError(message) => message,
        };
        
        utils::format_error(fmt, "syntax error", Some(message), self.source())
    }
}


// Structures used by the parser for error handling and synchronization

#[derive(Debug)]
pub struct ErrorContext {
    stack: Vec<ContextFrame>,
}

impl<'m> ErrorContext {
    pub fn new(base: ContextTag) -> Self {
        ErrorContext {
            stack: vec![ ContextFrame::new(base) ],
        }
    }
    
    pub fn frame(&self) -> &ContextFrame { self.stack.last().unwrap() }
    pub fn frame_mut(&mut self) -> &mut ContextFrame { self.stack.last_mut().unwrap() }
    
    pub fn push(&mut self, tag: ContextTag) { self.stack.push(ContextFrame::new(tag)) }
    
    pub fn push_continuation(&mut self, tag: ContextTag) {
        let start = self.frame().start().map(|o| o.to_owned());
        self.push(tag);
        self.frame_mut().set_span(start, None);
    }
    
    pub fn pop(&mut self) -> ContextFrame { 
        assert!(self.stack.len() > 1);
        self.stack.pop().unwrap()
    }
    
    pub fn pop_extend(&mut self) {
        let inner_frame = self.pop();
        self.frame_mut().extend(inner_frame);
    }
    
    pub fn take(mut self) -> ContextFrame {
        assert!(!self.stack.is_empty());
        self.stack.pop().unwrap()
    }
    
    // for convenience
    pub fn context(&self) -> ContextTag { self.frame().context() }
    pub fn set_start(&mut self, token: &TokenMeta) { self.frame_mut().set_start(token) }
    pub fn set_end(&mut self, token: &TokenMeta) { self.frame_mut().set_end(token) }
    
    pub fn take_debug_symbol(mut self) -> DebugSymbol {
        let mut symbol = self.frame().as_debug_symbol();
        while symbol.is_none() {
            if self.stack.len() <= 1 {
                symbol = self.take().as_debug_symbol();
                break;
            }
            
            self.pop();
            symbol = self.frame().as_debug_symbol();
        }
        
        symbol.expect("could not take debug symbol")
    }
}

#[derive(Debug)]
pub struct ContextFrame {
    tag: ContextTag,
    start: Option<Span>,
    end: Option<Span>,
}

fn span_lt(first: &Span, second: &Span) -> bool { first.index < second.index }
// fn span_min<'m>(first: &'m Span, second: &'m Span) -> &'m Span {
//     if span_lt(first, second) { first } else { second }
// }
// fn span_max<'m>(first: &'m Span, second: &'m Span) -> &'m Span {
//     if !span_lt(first, second) { first } else { second }
// }

impl ContextFrame {
    pub fn new(tag: ContextTag) -> Self { ContextFrame { tag, start: None, end: None } }
    
    pub fn context(&self) -> ContextTag { self.tag }
    pub fn start(&self) -> Option<&Span> { self.start.as_ref() }
    pub fn end(&self) -> Option<&Span> { self.end.as_ref() }
    
    pub fn set_start(&mut self, token: &TokenMeta) { 
        self.start.replace(token.span); 
    }
    
    pub fn set_end(&mut self, token: &TokenMeta) { 
        self.end.replace(token.span); 
    }
    
    pub fn set_span(&mut self, start: Option<Span>, end: Option<Span>) {
        self.start = start;
        self.end = end;
    }
    
    pub fn extend(&mut self, other: ContextFrame) {
        if self.start.as_ref().and(other.start.as_ref()).is_some() {
            if span_lt(other.start.as_ref().unwrap(), self.start.as_ref().unwrap()) {
                self.start = other.start;
            }
        } else if other.start.is_some() {
            self.start = other.start;
        }
        
        if self.end.as_ref().and(other.end.as_ref()).is_some() {
            if span_lt(self.end.as_ref().unwrap(), other.end.as_ref().unwrap()) {
                self.end = other.end;
            }
        } else if other.end.is_some() {
            self.end = other.end;
        }
    }
    
    pub fn as_debug_symbol(&self) -> Option<DebugSymbol> {
        match (self.start, self.end) {
            
            (Some(start), Some(end)) => {
                let start_index = start.index;
                let end_index = end.index + TokenIndex::from(end.length);
                Some((start_index, end_index).into())
            },
            
            (Some(span), None) | (None, Some(span)) => {
                Some((&span).into())
            },
            
            (None, None) => None,
        }
    }
}


