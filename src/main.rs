#[cfg_attr(test, macro_use)]
extern crate structopt;

use crate::logging::LogLevel::{Debug, Info, Trace};
use compression::CompressionFlag;
use encryption::EncryptionFlag;
use error::RustfoilError;
use gdrive::GDriveService;
use index::FileEntry;
use index::Index;
use index::ParsedFileInfo;
use indicatif::{ProgressBar, ProgressStyle};
use logging::Logger;
use regex::Regex;
use std::path::PathBuf;
use structopt::StructOpt;
use tinfoil::convert_to_tinfoil_format;

mod compression;
mod encryption;
mod error;
mod gdrive;
mod index;
mod logging;
mod result;
mod tinfoil;

/// Script that will allow you to generate an index file with Google Drive file links for use with Tinfoil
#[derive(StructOpt, Debug)]
#[structopt(rename_all = "kebab-case")]
pub struct Input {
    /// Folder IDs of Google Drive folders to scan
    folder_ids: Vec<String>,

    /// Path to Google Application Credentials
    #[structopt(long, parse(from_os_str), default_value = "credentials.json")]
    credentials: PathBuf,

    /// Path to Google OAuth2.0 User Token
    #[structopt(long, parse(from_os_str), default_value = "token.json")]
    token: PathBuf,

    /// Path to output index file
    #[structopt(short = "o", long, parse(from_os_str), default_value = "index.tlf")]
    output_path: PathBuf,

    /// Share all files inside the index file
    #[structopt(long)]
    share_files: bool,

    /// Scans for files only in top directory for each Folder ID entered
    #[structopt(long)]
    no_recursion: bool,

    /// Adds files without valid Title ID
    #[structopt(long)]
    add_nsw_files_without_title_id: bool,

    /// Adds files without valid NSW ROM extension(NSP/NSZ/XCI/XCZ) to index
    #[structopt(long)]
    add_non_nsw_files: bool,

    /// Adds a success message to index file to show if index is successfully read by Tinfoil
    #[structopt(long)]
    success: Option<String>,

    /// Adds a referrer to index file to prevent others from hotlinking
    #[structopt(long)]
    referrer: Option<String>,

    /// Adds a google API key to be used with all gdrive:/ requests
    #[structopt(long)]
    google_api_key: Option<String>,

    /// Adds 1Fincher API keys to be used with all 1f:/ requests, If multiple keys are provided, Tinfoil keeps trying them until it finds one that works
    #[structopt(long)]
    one_fichier_keys: Option<Vec<String>>,

    /// Adds custom HTTP headers Tinfoil should send with its requests
    #[structopt(long)]
    headers: Option<Vec<String>>,

    /// Adds a minimum Tinfoil version to load the index
    #[structopt(long)]
    min_version: Option<String>,

    /// Adds a list of themes to blacklist based on their hash
    #[structopt(long)]
    theme_blacklist: Option<Vec<String>>,

    /// Adds a list of themes to whitelist based on their hash
    #[structopt(long)]
    theme_whitelist: Option<Vec<String>>,

    /// Adds a custom theme error message to the index
    #[structopt(long)]
    theme_error: Option<String>,

    /// Path to RSA Public Key to encrypt AES-ECB-256 key with
    #[structopt(long)]
    public_key: Option<PathBuf>,

    /// Shares the index file that is uploaded to Google Drive
    #[structopt(long)]
    share_index: bool,

    /// If the index file should be uploaded to specific folder
    #[structopt(long)]
    upload_folder_id: Option<String>,

    /// If the index file should be uploaded to My Drive
    #[structopt(long)]
    upload_my_drive: bool,

    /// Which compression should be used for the index file
    #[structopt(long, default_value = "zstd")]
    compression: CompressionFlag,

    /// Verbose mode (-v, -vv, -vvv, etc.)
    #[structopt(short, long, parse(from_occurrences))]
    verbose: u8,
}

pub struct RustfoilService {
    logger: Logger,
    input: Input,
    gdrive: GDriveService,
}

impl RustfoilService {
    pub fn new(input: Input) -> RustfoilService {
        let credentials = input.credentials.clone();
        let token = input.token.clone();
        let verbose_count = input.verbose;
        RustfoilService {
            input,
            logger: Logger::new(match verbose_count {
                1 => Debug,
                2 => Trace,
                _ => Info,
            }),
            gdrive: GDriveService::new(credentials.as_path(), token.as_path()),
        }
    }

    pub fn validate_input(&self) -> std::result::Result<(), RustfoilError> {
        if !&self.input.credentials.exists() {
            return Err(RustfoilError::CredentialsMissing);
        }

        Ok(())
    }

    pub fn generate_index(&mut self, files: Vec<ParsedFileInfo>) -> result::Result<Box<Index>> {
        let mut index = Box::new(Index::new());

        let mut index_files: Vec<FileEntry> = Vec::new();

        for info in files {
            index_files.push(FileEntry::new(
                format!("gdrive:{}#{}", info.id, info.name_encoded),
                u64::from_str_radix(&*info.size, 10)?,
            ));
        }

        index.files = Some(index_files);

        self.logger.log_debug("Added files to index")?;

        if self.input.success.is_some() {
            index.success = Some(self.input.success.clone().unwrap());
            self.logger.log_debug("Added success message to index")?;
        }

        if self.input.referrer.is_some() {
            index.referrer = Some(self.input.referrer.clone().unwrap());
            self.logger.log_debug("Added referrer to index")?;
        }

        if self.input.google_api_key.is_some() {
            index.google_api_key = Some(self.input.google_api_key.clone().unwrap());
            self.logger.log_debug("Added google api key to index")?;
        }

        if self.input.one_fichier_keys.is_some() {
            index.one_fichier_keys = Some(self.input.one_fichier_keys.clone().unwrap());
            self.logger.log_debug("Added 1Fichier keys to index")?;
        }

        if self.input.headers.is_some() {
            index.headers = Some(self.input.headers.clone().unwrap());
            self.logger.log_debug("Added headers to index")?;
        }

        if self.input.min_version.is_some() {
            index.version = Some(self.input.min_version.clone().unwrap());
            self.logger.log_debug("Added minimum version to index")?;
        }

        if self.input.theme_blacklist.is_some() {
            index.theme_blacklist = Some(self.input.theme_blacklist.clone().unwrap());
            self.logger.log_debug("Added theme blacklist to index")?;
        }

        if self.input.theme_whitelist.is_some() {
            index.theme_whitelist = Some(self.input.theme_whitelist.clone().unwrap());
            self.logger.log_debug("Added theme whitelist to index")?;
        }

        if self.input.theme_error.is_some() {
            index.theme_error = Some(self.input.theme_error.clone().unwrap());
            self.logger
                .log_debug("Added theme error message to index")?;
        }

        self.logger.log_info("Generated index successfully")?;

        Ok(index)
    }

    pub fn output_index(&self, index: Index) -> result::Result<()> {
        let json = serde_json::to_string(&index)?;
        let compression = &self.input.compression;
        let encryption_file_path_buf = self.input.public_key.clone();
        let encryption = if self.input.public_key.is_some() {
            EncryptionFlag::Encrypt
        } else {
            EncryptionFlag::NoEncrypt
        };

        std::fs::write(
            &self.input.output_path,
            convert_to_tinfoil_format(
                json.as_str(),
                compression.clone(),
                encryption.clone(),
                encryption_file_path_buf,
            )?,
        )
        .expect("Couldn't write output file to Path");

        self.logger.log_info(
            format!(
                "Finished writing {} to disk, using {} compression & {}encryption",
                self.input
                    .output_path
                    .file_name()
                    .unwrap()
                    .to_str()
                    .unwrap(),
                compression = match compression {
                    CompressionFlag::Off => "no".to_string(),
                    CompressionFlag::ZSTD | CompressionFlag::Zlib => {
                        compression.to_string()
                    }
                },
                encryptiom = match encryption {
                    EncryptionFlag::NoEncrypt => "no ",
                    EncryptionFlag::Encrypt => "",
                }
            )
            .as_str(),
        )?;

        Ok(())
    }

    pub fn share_file(&self, file_id: String, is_shared: bool) {
        if !is_shared {
            self.gdrive.share_file(file_id.as_str());
        }
    }

    pub fn share_files(&self, files: Vec<ParsedFileInfo>) -> result::Result<()> {
        let pb = ProgressBar::new(files.len() as u64);

        pb.set_style(
            ProgressStyle::default_bar()
                .template("[{elapsed_precise}] [{bar:40.cyan/blue}] {msg} {pos:>7}/{len:7} Files")
                .progress_chars("#>-"),
        );

        pb.set_message("Sharing");

        for file in files {
            let parsed_file_clone = file.clone();
            self.share_file(parsed_file_clone.id, parsed_file_clone.shared);
            pb.inc(1);
        }

        pb.finish_with_message("Finished Sharing");

        Ok(())
    }

    pub fn upload_index(&self) -> std::io::Result<(String, bool)> {
        let folder_id = self.input.upload_folder_id.clone();
        let input = self.input.output_path.as_path();

        let res = self.gdrive.upload_file(input, folder_id.clone()).unwrap();

        self.logger.log_info(
            format!(
                "Uploaded Index to {}",
                destination = folder_id.unwrap_or("My Drive".to_string())
            )
            .as_str(),
        )?;

        Ok(res)
    }

    pub fn scan_folder(&mut self) -> result::Result<Vec<ParsedFileInfo>> {
        let re = Regex::new("%5B[0-9A-Fa-f]{16}%5D")?;

        // Trigger Authentication if needed
        self.gdrive.trigger_auth();

        let pb = ProgressBar::new(!0);
        pb.enable_steady_tick(130);
        pb.set_style(
            ProgressStyle::default_spinner()
                // For more spinners check out the cli-spinners project:
                // https://github.com/sindresorhus/cli-spinners/blob/master/spinners.json
                .tick_strings(&["-", "\\", "|", "/"])
                .template("{spinner:.blue} {msg}"),
        );
        pb.set_message("Scanning...");

        let files: Vec<ParsedFileInfo> = self
            .input
            .folder_ids
            .iter()
            .map(|id| -> Vec<ParsedFileInfo> {
                self.gdrive
                    .get_all_files_in_folder(id.to_owned().as_str(), !self.input.no_recursion)
                    .unwrap()
                    .into_iter()
                    .map(|file_info| ParsedFileInfo::new(file_info))
                    .filter(|file| {
                        let mut keep = true;

                        if !self.input.add_non_nsw_files {
                            let extension: String = file
                                .name
                                .chars()
                                .skip(file.name.len() - 4)
                                .take(4)
                                .collect();

                            keep = vec![".nsp", ".nsz", ".xci", ".xcz"].contains(&&*extension);
                        }

                        if !self.input.add_nsw_files_without_title_id {
                            keep = re.is_match(file.name_encoded.as_str());
                        }

                        keep
                    })
                    .collect()
            })
            .flatten()
            .collect();

        pb.finish_with_message(&*format!("Scanned {} files", files.len()));

        Ok(files)
    }
}

pub fn main() {
    match real_main() {
        Ok(_) => std::process::exit(0),
        Err(_) => std::process::exit(1),
    }
}

fn real_main() -> result::Result<()> {
    // TODO: do validate checks before or move gdrive hub construction to later point so it doesn't trigger panics when credentials are missing
    let mut service = RustfoilService::new(Input::from_args());

    service.validate_input()?;

    let files = service.scan_folder()?;

    let index = service.generate_index(files.to_owned())?;

    service.output_index(*index)?;

    if service.input.share_files {
        service.share_files(files)?;
    }

    if service.input.upload_my_drive || service.input.upload_folder_id.is_some() {
        let (id, shared) = service.upload_index()?;

        if service.input.share_index {
            service.share_file(id, shared);
            service.logger.log_info("Shared Index File")?;
        }
    };

    Ok(())
}
