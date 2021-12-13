use std::path::{Path, PathBuf};
use std::io::Write;
use std::fs;
use std::env;
use std::process::{Command, Stdio};

#[allow(unused_imports)]
use anyhow::{Result, anyhow, bail};
use cargo_toml::Manifest;
use syn::parse::Parser;
use syn_inline_mod::InlinerBuilder;
use quote::quote;

mod print;
use print::SynFilePrint;

fn inline_module(path: &Path) -> Result<syn::File> {
    // load the file as AST
    let (ast, errors) = InlinerBuilder::default()
        .parse_and_inline_modules(path)?
        .into_output_and_errors();

    for err in errors.into_iter() {
        bail!("Error when parsing {}, included by {} as mod {}: {}", err.path().display(), err.src_path().display(), err.module_name(), err.kind());
    }

    Ok(ast)
}

fn modulize_crate(name: &str, file: syn::File) -> Result<syn::ItemMod> {
    todo!()
}

fn new_manifest_comment(content: &str) -> Vec<syn::Attribute> {
    // first create a token stream using quote,
    let content = std::iter::once("```cargo")
        .chain(content.lines())
        .chain(std::iter::once("```"))
        .map(|line| format!(" {}", line));
    let attr = quote! {
        #(#![doc = #content])*
    };
    // and then parse back to syn::Attribute
    let attr = syn::Attribute::parse_inner
        .parse(attr.into())
        .expect("Just quoted input can not be wrong");
    attr
}

/// make the file a little readable
fn format_file(path: &Path) -> Result<()> {
    let status = Command::new("rustfmt")
        .arg(path)
        .stdin(Stdio::null())
        .status()?;
    if !status.success() {
        bail!("Failed to run rustfmt on {}", path.display());
    }
    Ok(())
}

pub struct Bundler {
    binary_path: PathBuf,
    crates: Vec<(String, PathBuf)>,

    out_dir: PathBuf,
    manifest: Manifest,
    /// also save content for later writing
    manifest_str: String,
}

impl Bundler {
    pub fn new(binary: &Path) -> Result<Self> {
        let out_dir = env::var_os("OUT_DIR").ok_or_else(|| anyhow!("Missing OUT_DIR env var"))?;
        let manifest_dir = env::var_os("CARGO_MANIFEST_DIR").ok_or_else(|| anyhow!("Missing CARGO_MANIFEST_DIR env var"))?;
        Self::new_with_dir(
            binary,
            out_dir,
            manifest_dir,
        )
    }

    pub fn new_with_dir(binary: impl Into<PathBuf>, out_dir: impl Into<PathBuf>, manifest_dir: impl Into<PathBuf>) -> Result<Self> {
        let manifest_path = manifest_dir.into().join("Cargo.toml");
        let manifest_str = fs::read_to_string(&manifest_path)?;
        let mut manifest = Manifest::from_str(&manifest_str)?;
        manifest.complete_from_path(&manifest_path)?;
        Ok(Bundler {
            binary_path: binary.into(),
            crates: Default::default(),

            out_dir: out_dir.into(),
            manifest,
            manifest_str,
        })
    }

    pub fn with_lib(mut self) -> Self {
        if let Some((name, path)) = self.manifest.lib.as_ref()
            .and_then(|lib| lib.name.as_ref().zip(lib.path.as_ref())) {
            self.crates.push((name.into(), path.into()));
        }
        self
    }

    pub fn with_crate_at(mut self, name: impl Into<String>, root: impl Into<PathBuf>) -> Self {
        self.crates.push((name.into(), root.into()));
        self
    }

    /// Expand a binary rs file to `target`, which is relative to `OUT_DIR`.
    /// Also write a rust-script compatible header and vim file type footer.
    pub fn bundle(self, target: &Path) -> Result<PathBuf> {
        let target = self.out_dir.join(target);

        // parse the binary
        let mut binary = inline_module(&self.binary_path)?;

        // parse any crate, also modulize them
        let libs = self.crates
            .into_iter()
            .map(|(name, path)| {
                let lib = inline_module(&path)?;
                let lib = modulize_crate(&name, lib)?;
                Ok(lib)
            })
            .collect::<Result<Vec<_>>>()?;

        // add libs to binary
        binary.items.extend(libs.into_iter().map(Into::into));

        // add rust-script shebang
        binary.shebang = Some("#!/usr/bin/env -S rust-script".into());
        // add doc attribute for cargo manifest, make sure we add to the head
        let _: Vec<_> = binary.attrs
            .splice(..0, new_manifest_comment(&self.manifest_str))
            .collect();

        // print the file
        {
            let mut bundle = fs::File::create(&target)?;

            writeln!(bundle, "{}", binary.print())?;

            // write the footer
            writeln!(bundle, "// vim: ft=rust syntax=rust")?;
        }

        // make it readable
        format_file(&target)?;

        Ok(target)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        let result = 2 + 2;
        assert_eq!(result, 4);
    }
}
