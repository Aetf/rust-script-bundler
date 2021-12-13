use std::fmt::Display;

use proc_macro2::{Delimiter, Spacing, TokenStream, TokenTree};
use quote::ToTokens;
use syn::Lit;

pub trait SynFilePrint {
    fn print(&self) -> FilePrinter;
}

impl SynFilePrint for syn::File {
    fn print(&self) -> FilePrinter {
        FilePrinter(&self)
    }
}

pub struct FilePrinter<'a>(&'a syn::File);

impl<'a> Display for FilePrinter<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let file = self.0;
        if let Some(shebang) = &file.shebang {
            writeln!(f, "{}", shebang)?;
        }

        // write inner attributes, we do two passes,
        // first are all doc attributes
        for attr in file.attrs.iter().filter(|a| a.path.is_ident("doc")) {
            assert!(
                matches!(attr.style, syn::AttrStyle::Inner(_)),
                "File can only have inner attributes at top level"
            );
            writeln!(f, "//!{}", attr.tokens)?;
        }
        // then others
        for attr in file.attrs.iter().filter(|a| !a.path.is_ident("doc")) {
            assert!(
                matches!(attr.style, syn::AttrStyle::Inner(_)),
                "File can only have inner attributes at top level"
            );
            writeln!(f, "#![{}{}]", attr.path.to_token_stream(), attr.tokens)?;
        }

        // write items as is
        for item in file.items.iter() {
            write_tokens_normalized(f, item.to_token_stream())?;
            writeln!(f, "\n")?;
        }

        Ok(())
    }
}

/// Write tokens same way as `TokenStream::to_string` would do, but with normalization of doc
/// attributes into `///`.
///
/// Adapted from sourcegen cli @ commit 1492a97e86eee5e69a959c4347efb3c8c58e1a7e
/// https://github.com/commure/sourcegen
fn write_tokens_normalized(f: &mut std::fmt::Formatter, tokens: TokenStream) -> std::fmt::Result {
    let mut tokens = tokens.into_iter().peekable();
    let mut joint = false;
    let mut first = true;
    while let Some(tt) = tokens.next() {
        if !first && !joint {
            write!(f, " ")?;
        }
        first = false;
        joint = false;

        // normalize doc attributes
        if let Some(comment) = tokens
            .peek()
            .and_then(|lookahead| as_doc_comment(&tt, lookahead))
        {
            let _ignore = tokens.next();
            writeln!(f, "///{}", comment)?;
            continue;
        }
        // write tt recursively
        match tt {
            TokenTree::Group(ref tt) => {
                let (start, end) = match tt.delimiter() {
                    Delimiter::Parenthesis => ("(", ")"),
                    Delimiter::Brace => ("{\n", "}\n"),
                    Delimiter::Bracket => ("[", "]"),
                    Delimiter::None => ("", ""),
                };
                if tt.stream().into_iter().next().is_none() {
                    write!(f, "{} {}", start, end)?
                } else {
                    write!(f, "{} ", start)?;
                    write_tokens_normalized(f, tt.stream())?;
                    write!(f, " {}\n", end)?
                }
            }
            TokenTree::Ident(ref tt) => write!(f, "{}", tt)?,
            TokenTree::Punct(ref tt) => {
                let ch = tt.as_char();
                write!(f, "{}", ch)?;
                if ch == ';' {
                    write!(f, "\n")?;
                }
                match tt.spacing() {
                    Spacing::Alone => {}
                    Spacing::Joint => joint = true,
                }
            }
            TokenTree::Literal(ref tt) => write!(f, "{}", tt)?,
        }
    }
    Ok(())
}

/// Adapted from sourcegen cli @ commit 1492a97e86eee5e69a959c4347efb3c8c58e1a7e
/// https://github.com/commure/sourcegen
fn as_doc_comment(first: &TokenTree, second: &TokenTree) -> Option<String> {
    match (first, second) {
        (TokenTree::Punct(first), TokenTree::Group(group))
            if first.as_char() == '#' && group.delimiter() == Delimiter::Bracket =>
        {
            let mut it = group.stream().into_iter();
            match (it.next(), it.next(), it.next()) {
                (
                    Some(TokenTree::Ident(ident)),
                    Some(TokenTree::Punct(punct)),
                    Some(TokenTree::Literal(lit)),
                ) => {
                    if ident == "doc" && punct.as_char() == '=' {
                        if let Lit::Str(lit) = Lit::new(lit) {
                            return Some(lit.value());
                        }
                    }
                }
                _ => {}
            }
        }
        _ => {}
    }
    None
}
