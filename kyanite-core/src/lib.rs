use std::{fmt, fs::File, io::Read, path::Path};

use codegen::IrError;

use crate::{
    codegen::Ir,
    pass::{SymbolTable, TypeCheckPass},
};

pub use compile::Compile;

mod ast;
mod codegen;
mod compile;
mod parse;
mod pass;
mod reporting;
pub mod subprocess;
mod token;

#[derive(thiserror::Error, Debug)]
pub enum PipelineError {
    #[error("file \"{0}\" does not exist")]
    FileNotFound(String),
    #[error("File is not valid UTF-8")]
    InvalidUtf8,
    #[error("(while lexing source) {0} error(s) encountered")]
    LexError(usize),
    #[error("(while parsing) {0} error(s) encountered")]
    ParseError(usize),
    #[error("(while type checking) {0} error(s) encountered")]
    TypeError(usize),
    #[error("(while building ir) {0}")]
    IrError(IrError),
    #[error("failed to compile (see output)")]
    CompileError(String),
}

#[derive(Debug)]
pub struct Program {
    filename: String,
    ast: ast::Ast,
    ir: String,
}

impl Program {
    pub fn from_file<P>(path: P) -> Result<Self, PipelineError>
    where
        P: AsRef<Path>,
    {
        let source = Source::new(path)?;
        let ast = ast::Ast::from_source(source.clone())?;
        Self::new(ast, &source)
    }

    pub fn from_string(str: String) -> Result<Self, PipelineError> {
        let dir = Path::new("/tmp/com.github.alaidriel/kyanite");
        if !dir.exists() {
            std::fs::create_dir_all(dir).expect("permission to write to /tmp");
        }
        let filename = dir
            .join("main.kya")
            .to_str()
            .ok_or(PipelineError::InvalidUtf8)?
            .to_string();
        std::fs::write(&filename, format!("defn main() {{\n\t{}\n}}", str))
            .map_err(|_| PipelineError::FileNotFound(filename.clone()))?;
        let source = Source::new(filename)?;
        let ast = ast::Ast::from_source(source.clone())?;
        Self::new(ast, &source)
    }

    fn new(mut ast: ast::Ast, source: &Source) -> Result<Self, PipelineError> {
        fn strip_prefix(filename: &str) -> String {
            let chars: Vec<_> = filename.chars().collect();
            let name: Vec<_> = chars.iter().rev().take_while(|&&c| c != '/').collect();
            name.iter().rev().copied().copied().collect()
        }

        let symbols = SymbolTable::from(&ast.file);
        let mut pass = TypeCheckPass::new(symbols, source, &ast.file);
        pass.run().map_err(PipelineError::TypeError)?;
        Ok(Self {
            ir: Ir::from_ast(&mut ast).map_err(PipelineError::IrError)?,
            filename: strip_prefix(&source.filename),
            ast,
        })
    }
}

impl fmt::Display for Program {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.ast.file)
    }
}

#[derive(Debug, Default, Clone)]
pub struct Source {
    filename: String,
    chars: Vec<char>,
    raw: String,
}

impl Source {
    pub fn new<P>(path: P) -> Result<Self, PipelineError>
    where
        P: AsRef<Path>,
    {
        let filename = path
            .as_ref()
            .to_str()
            .ok_or(PipelineError::InvalidUtf8)?
            .to_string();
        let mut raw = String::new();
        let mut file =
            File::open(&path).map_err(|_| PipelineError::FileNotFound(filename.clone()))?;
        file.read_to_string(&mut raw)
            .map_err(|_| PipelineError::InvalidUtf8)?;
        Ok(Self {
            filename,
            chars: raw.chars().collect(),
            raw,
        })
    }
}
