use std::env;
use std::path::{PathBuf, Path};

use anyhow::{Result, bail};
use rust_script_bundler::Bundler;

fn main() {
    if let Err(e) = try_main() {
        eprintln!("{:?}", e);
    }
}

fn try_main() -> Result<()> {
    let args = env::args_os()
        .skip(1)
        .map(PathBuf::from)
        .collect::<Vec<_>>();

    let (crate_path, bin_path, target_path) = match args[..] {
        [ref c, ref b, ref t] => (c, b, t),
        _ => bail!("Incorrect usage"),
    };

    Bundler::new_with_dir(bin_path, target_path.parent().unwrap(), crate_path)?
        .bundle(Path::new(target_path.file_name().unwrap()))?;

    Ok(())
}
