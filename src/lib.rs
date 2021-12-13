use std::env;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[allow(unused_imports)]
use anyhow::{anyhow, bail, Result, Context};
use cargo_toml::Manifest;
use quote::quote;
use syn::parse::Parser;
use syn_inline_mod::InlinerBuilder;

mod print;
use print::SynFilePrint;

fn inline_module(path: &Path) -> Result<syn::File> {
    // load the file as AST
    let (ast, errors) = InlinerBuilder::default()
        .parse_and_inline_modules(path)
        .with_context(|| format!("Failed to parse and inline modules at {}", path.display()))?
        .into_output_and_errors();

    for err in errors.into_iter() {
        bail!(
            "Error when parsing {}, included by {} as mod {}: {}",
            err.path().display(),
            err.src_path().display(),
            err.module_name(),
            err.kind()
        );
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
        .parse2(attr)
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

    manifest: Manifest,
    /// also save content for later writing
    manifest_str: String,

    out_dir: PathBuf,
}

impl Bundler {
    pub fn new(binary: impl AsRef<Path>) -> Result<Self> {
        let out_dir = env::var_os("OUT_DIR").ok_or_else(|| anyhow!("Missing OUT_DIR env var"))?;
        let manifest_dir = env::var_os("CARGO_MANIFEST_DIR")
            .ok_or_else(|| anyhow!("Missing CARGO_MANIFEST_DIR env var"))?;
        Self::new_with_dir(binary, out_dir, manifest_dir)
    }

    pub fn new_with_dir(
        binary: impl AsRef<Path>,
        out_dir: impl Into<PathBuf>,
        manifest_dir: impl Into<PathBuf>,
    ) -> Result<Self> {
        let manifest_dir = manifest_dir.into();

        let manifest_path = manifest_dir.join("Cargo.toml");
        let manifest_str = fs::read_to_string(&manifest_path)
            .with_context(|| format!("Failed to read manifest at {}", &manifest_path.display()))?;
        let mut manifest = Manifest::from_str(&manifest_str)?;
        manifest.complete_from_path(&manifest_path)?;

        Ok(Bundler {
            binary_path: manifest_dir.join(binary.as_ref()),
            crates: Default::default(),

            manifest,
            manifest_str,

            out_dir: out_dir.into(),
        })
    }

    pub fn with_lib(mut self) -> Self {
        if let Some((name, path)) = self
            .manifest
            .lib
            .as_ref()
            .and_then(|lib| lib.name.as_ref().zip(lib.path.as_ref()))
        {
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
        if let Some(p) = target.parent() {
            fs::create_dir_all(p).context("failed to create out dir")?;
        }

        // parse the binary
        let mut binary = inline_module(&self.binary_path)?;

        // parse any crate, also modulize them
        let libs = self
            .crates
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
        let _: Vec<_> = binary
            .attrs
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
        // format_file(&target)?;

        Ok(target)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_manifest_comment_works() {
        let attrs = new_manifest_comment("abc\n  def");

        dbg!(attrs);
    }
}
