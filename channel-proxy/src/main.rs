extern crate fs_extra;
extern crate iron;
#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate router;
extern crate tempfile;

use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};
use std::{env, fmt, fs};

#[derive(Clone, Debug)]
struct Config {
  upstream_channel_url: String,
  persistent_nixexprs_path: PathBuf,
  build_expression: Option<String>,
}

impl Config {
  fn from_env() -> Config {
    Config {
      upstream_channel_url: env::var("NIX_CHANNEL_PROXY_UPSTREAM_CHANNEL_URL")
        .unwrap_or("https://nixos.org/channels/nixos-19.03".into()),
      persistent_nixexprs_path: env::var("NIX_CHANNEL_PROXY_PERSISTENT_NIXEXPRS_PATH")
        .unwrap_or("/tmp/nixexprs.tar.xz".into())
        .into(),
      build_expression: env::var("NIX_CHANNEL_PROXY_BUILD_EXPRESSION").ok(),
    }
  }
}

lazy_static! {
  static ref CONFIG: Config = Config::from_env();
}

enum AppError {
  FailedToCreateTemporaryDirectory(::std::io::Error),
  FailedToDownloadError(String, PathBuf, ::std::io::Error),
  FailedToDownloadStatus(String, PathBuf, ExitStatus),
  FailedToCreateUnpackDirectory(PathBuf, ::std::io::Error),
  FailedToUnpackError(PathBuf, PathBuf, ::std::io::Error),
  FailedToUnpackStatus(PathBuf, PathBuf, ExitStatus),
  FailedToReadUnpackDir(PathBuf, ::std::io::Error),
  UnpackedTooFewEntries(PathBuf),
  UnpackedTooManyEntries(PathBuf),
  FailedToBuildError(::std::io::Error),
  FailedToBuildStatus(ExitStatus),
  FailedToRename(PathBuf, PathBuf, ::fs_extra::error::Error),
  FailedToStartServer(::iron::error::HttpError),
}

impl fmt::Display for AppError {
  fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
    match *self {
      AppError::FailedToCreateTemporaryDirectory(ref err) => {
        write!(formatter, "failed to create temporary directory: {}", err)
      }
      AppError::FailedToDownloadError(ref url, ref dest, ref err) => write!(
        formatter,
        "failed to download {} to {}: {}",
        url,
        dest.display(),
        err
      ),
      AppError::FailedToDownloadStatus(ref url, ref dest, ref status) => write!(
        formatter,
        "failed to download {} to {}: child process exited with status {}",
        url,
        dest.display(),
        status
      ),
      AppError::FailedToCreateUnpackDirectory(ref path, ref err) => {
        write!(formatter, "failed to create {}: {}", path.display(), err)
      }
      AppError::FailedToUnpackError(ref file, ref dir, ref err) => write!(
        formatter,
        "failed to unpack {} into {}: {}",
        file.display(),
        dir.display(),
        err
      ),
      AppError::FailedToUnpackStatus(ref file, ref dir, ref status) => write!(
        formatter,
        "failed to unpack {} into {}: child process exited with status {}",
        file.display(),
        dir.display(),
        status
      ),
      AppError::FailedToReadUnpackDir(ref dir, ref err) => write!(
        formatter,
        "failed to read directory {}: {}",
        dir.display(),
        err
      ),
      AppError::UnpackedTooFewEntries(ref dir) => write!(
        formatter,
        "unpack produced too few files in {}",
        dir.display()
      ),
      AppError::UnpackedTooManyEntries(ref dir) => write!(
        formatter,
        "unpack produced too many files in {}",
        dir.display()
      ),
      AppError::FailedToBuildError(ref err) => {
        write!(formatter, "failed to pre-build targets: {}", err)
      }
      AppError::FailedToBuildStatus(ref status) => write!(
        formatter,
        "failed to pre-build targets: child process exited with status {}",
        status
      ),
      AppError::FailedToRename(ref src, ref dest, ref err) => write!(
        formatter,
        "failed to rename {} to {}: {}",
        src.display(),
        dest.display(),
        err
      ),
      AppError::FailedToStartServer(ref err) => {
        write!(formatter, "failed to start HTTP server: {}", err)
      }
    }
  }
}

fn get_nixexprs_url(channel_url: &str) -> String {
  let mut nixexprs_url = channel_url.to_string();
  if !nixexprs_url.ends_with('/') {
    nixexprs_url.push('/');
  }
  nixexprs_url.push_str("nixexprs.tar.xz");
  nixexprs_url
}

fn download_nixexprs(url: &str, dest: &Path) -> Result<(), AppError> {
  println!("downloading {}", url);
  let status = Command::new("curl")
    .arg("--location")
    .arg("--silent")
    .arg("--show-error")
    .arg("--output")
    .arg(dest.to_str().unwrap())
    .arg(url)
    .status()
    .map_err(|e| AppError::FailedToDownloadError(url.to_string(), dest.to_path_buf(), e))?;
  if status.success() {
    Ok(())
  } else {
    Err(AppError::FailedToDownloadStatus(
      url.to_string(),
      dest.to_path_buf(),
      status,
    ))
  }
}

fn unpack_nixexprs(file: &Path, dest_dir: &Path) -> Result<PathBuf, AppError> {
  println!("unpacking {}", file.display());
  fs::create_dir(dest_dir)
    .map_err(|e| AppError::FailedToCreateUnpackDirectory(dest_dir.to_path_buf(), e))?;
  let status = Command::new("tar")
    .arg("--extract")
    .arg("--xz")
    .arg("--file")
    .arg(file.to_str().unwrap())
    .arg("--directory")
    .arg(dest_dir.to_str().unwrap())
    .status()
    .map_err(|e| AppError::FailedToUnpackError(file.to_path_buf(), dest_dir.to_path_buf(), e))?;
  if status.success() {
    let mut entries = fs::read_dir(dest_dir)
      .map_err(|e| AppError::FailedToReadUnpackDir(dest_dir.to_path_buf(), e))?;
    match (entries.next(), entries.next()) {
      (None, _) => Err(AppError::UnpackedTooFewEntries(dest_dir.to_path_buf())),
      (Some(Ok(entry)), None) => Ok(entry.path()),
      (Some(Err(e)), None) => Err(AppError::FailedToReadUnpackDir(dest_dir.to_path_buf(), e)),
      (Some(_), Some(_)) => Err(AppError::UnpackedTooManyEntries(dest_dir.to_path_buf())),
    }
  } else {
    Err(AppError::FailedToUnpackStatus(
      file.to_path_buf(),
      dest_dir.to_path_buf(),
      status,
    ))
  }
}

fn build(nixpkgs: &Path, expression: &str) -> Result<(), AppError> {
  println!("pre-building targets");
  let status = Command::new("nix-build")
    .arg("--no-out-link")
    .arg("-I")
    .arg(format!("nixpkgs={}", nixpkgs.display()))
    .arg("--expr")
    .arg(expression)
    .status()
    .map_err(|e| AppError::FailedToBuildError(e))?;
  if status.success() {
    Ok(())
  } else {
    Err(AppError::FailedToBuildStatus(status))
  }
}

fn deploy(src: &Path, dest: &Path) -> Result<(), AppError> {
  println!("installing {} to {}", src.display(), dest.display());
  use fs_extra::file::{move_file, CopyOptions};
  let mut opts = CopyOptions::new();
  opts.overwrite = true;
  move_file(src, dest, &opts)
    .map_err(|e| AppError::FailedToRename(src.to_path_buf(), dest.to_path_buf(), e))
    .map(drop)
}

fn update() -> Result<(), AppError> {
  let tmp = tempfile::tempdir().map_err(AppError::FailedToCreateTemporaryDirectory)?;
  let nixexprs_url = get_nixexprs_url(&CONFIG.upstream_channel_url);
  let nixexprs_file = tmp.path().join("nixexprs.tar.xz");
  let nixexprs_unpack_dir = tmp.path().join("unpack");
  download_nixexprs(&nixexprs_url, &nixexprs_file)?;
  match CONFIG.build_expression {
    Some(ref expr) => {
      let nixpkgs = unpack_nixexprs(&nixexprs_file, &nixexprs_unpack_dir)?;
      build(&nixpkgs, expr)?
    }
    None => (),
  }
  deploy(&nixexprs_file, &CONFIG.persistent_nixexprs_path)?;
  Ok(())
}

fn update_async() {
  ::std::thread::spawn(move || match update() {
    Ok(()) => (),
    Err(e) => println!("error: {}", e),
  });
}

fn serve() -> Result<(), AppError> {
  let addr = "0.0.0.0:8000";
  println!("listening on http://{}/", addr);
  iron::Iron::new(
    router!(
      channel_home: get "/channel" => move |_req: &mut iron::Request| {
        Ok(iron::Response::with(iron::status::Status::Ok))
      },
      nixexprs: get "/channel/nixexprs.tar.xz" => move |_req: &mut iron::Request| {
        Ok(iron::Response::with((iron::status::Status::Ok, CONFIG.persistent_nixexprs_path.as_path())))
      },
      upstream: get "/upstream" => move |_req: &mut iron::Request| {
        Ok(iron::Response::with((iron::status::Status::Ok, CONFIG.upstream_channel_url.as_str())))
      },
      update: post "/update" => move |_req: &mut iron::Request| {
        update_async();
        Ok(iron::Response::with(iron::status::Status::Accepted))
      },
    )
  ).http(addr).map_err(AppError::FailedToStartServer).map(drop)
}

fn main_inner() -> Result<(), AppError> {
  update_async();
  serve()?;
  Ok(())
}

fn main() {
  match main_inner() {
    Ok(_) => (),
    Err(e) => println!("error: {}", e),
  }
}
