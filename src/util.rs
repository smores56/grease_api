//! Extra utilties for use elsewhere in the API.

use base64::decode;
use chrono::{Local, NaiveDateTime};
use error::{GreaseError, GreaseResult};
use glob::glob;
use serde::Deserialize;
use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::str::FromStr;

pub struct Email<'e> {
    pub from_name: &'e str,
    pub from_address: &'e str,
    pub to_name: &'e str,
    pub to_address: &'e str,
    pub subject: &'e str,
    pub content: &'e str,
}

impl<'e> Email<'e> {
    pub fn send(&self) -> GreaseResult<()> {
        let email = format!(
            "To: {} <{}>\nFrom: {} <{}>\nSubject: {}\n{}\n.\n",
            self.to_name,
            self.to_address,
            self.from_name,
            self.from_address,
            self.subject,
            self.content
        );
        let mut sendmail = Command::new("sendmail")
            .stdin(Stdio::piped())
            .spawn()
            .map_err(|err| {
                GreaseError::ServerError(format!("Couldn't run sendmail to send an email: {}", err))
            })?;
        sendmail
            .stdin
            .as_mut()
            .ok_or(GreaseError::ServerError(
                "No stdin was available for sendmail.".to_owned(),
            ))?
            .write_all(email.as_bytes())
            .map_err(|err| {
                GreaseError::ServerError(format!("Couldn't send an email with sendmail: {}", err))
            })?;
        let output = sendmail.wait_with_output().map_err(|err| {
            GreaseError::ServerError(format!("sendmail failed to send an email: {}", err))
        })?;

        if output.status.success() {
            Ok(())
        } else {
            let error_message = std::str::from_utf8(&output.stderr).map_err(|_err| {
                GreaseError::ServerError(
                    "sendmail errored out with a non-utf8 error message.".to_owned(),
                )
            })?;
            Err(GreaseError::ServerError(format!(
                "sendmail failed to send an email: {}",
                error_message
            )))
        }
    }
}

#[derive(Deserialize, grease_derive::Extract)]
pub struct FileUpload {
    pub path: String,
    pub content: String,
}

impl FileUpload {
    pub fn upload(&self) -> GreaseResult<()> {
        let content = decode(&self.content).map_err(|err| {
            GreaseError::BadRequest(format!("couldn't decode file as base64: {}", err))
        })?;
        let path = {
            let given_path = PathBuf::from_str(&self.path).map_err(|_err| {
                GreaseError::BadRequest(format!("invalid file name: {}", &self.path))
            })?;
            let file_name = given_path.file_name().ok_or(GreaseError::BadRequest(
                "file name must end in an absolute path".to_owned(),
            ))?;
            let _extension = given_path.extension().ok_or(GreaseError::BadRequest(
                "file must have an extension".to_owned(),
            ))?;
            let mut base_path = PathBuf::from("./music/");
            base_path.push(file_name);

            base_path
        };
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(path)
            .map_err(|err| GreaseError::ServerError(format!("error opening file: {}", err)))?;
        file.write_all(&content)
            .map_err(|err| GreaseError::ServerError(format!("error writing to file: {}", err)))?;

        Ok(())
    }
}

pub fn check_for_music_file(path: &str) -> GreaseResult<String> {
    let given_path = PathBuf::from_str(path)
        .map_err(|_err| GreaseError::BadRequest(format!("invalid file name: {}", path)))?;
    let file_name = given_path
        .file_name()
        .ok_or(GreaseError::BadRequest(
            "file name must end in an absolute path".to_owned(),
        ))?
        .to_string_lossy()
        .to_string();
    let _extension = given_path.extension().ok_or(GreaseError::BadRequest(
        "file must have an extension".to_owned(),
    ))?;

    let mut existing_path = PathBuf::from("./music/");
    existing_path.push(&file_name);

    if std::fs::metadata(existing_path).is_ok() {
        Ok(file_name)
    } else {
        Err(GreaseError::BadRequest(format!(
            "the file {} doesn't exist yet and must be uploaded before a link to it can be made",
            file_name
        )))
    }
}

pub fn log_panic(request: &cgi::Request, error_message: String) -> cgi::Response {
    let now = Local::now().naive_local();
    let file_name = format!("./log/log {}.txt", now.format("%c"));
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(file_name)
        .expect("couldn't open new log file");
    let write_to_file = |file: &mut std::fs::File, content: String| {
        file.write_all(content.as_bytes())
            .expect("couldn't write to log file");
    };

    let headers = request
        .headers()
        .into_iter()
        .map(|(key, value)| (key.to_string(), value.to_str().unwrap().to_string()))
        .collect::<HashMap<String, String>>();
    write_to_file(
        &mut file,
        format!(
            "At {}, panicked during request handling.\n",
            now.format("%c")
        ),
    );
    write_to_file(&mut file, format!("Headers:\n  {:?}\n", headers));
    write_to_file(&mut file, format!("Request:\n  {:?}\n", request));
    write_to_file(
        &mut file,
        format!("Error generated:\n  {}\n", error_message),
    );

    clean_up_old_logs();

    let json_val = serde_json::json!({
        "message": "Panicked during handling of request. Please contact an administrator with the following information:",
        "time": now.format("%c").to_string(),
        "request": format!("{:?}", request),
        "error": error_message,
        "headers": headers,
    });
    let body = json_val.to_string().into_bytes();

    http::response::Builder::new()
        .status(500)
        .body(body)
        .unwrap()
}

fn clean_up_old_logs() {
    let log_files: Vec<PathBuf> = glob("./log/*.txt")
        .expect("Failed to read glob pattern")
        .collect::<Result<Vec<_>, _>>()
        .expect("one of the log files had an invalid name");
    if log_files.len() >= 50 {
        let mut log_times = log_files
            .iter()
            .map(|log_file: &PathBuf| {
                let file_name = log_file
                    .file_name()
                    .expect("no file name found for log file")
                    .to_string_lossy();
                let time = NaiveDateTime::parse_from_str(&file_name, "log %c")
                    .expect("log file was incorrectly named");
                (log_file, time)
            })
            .collect::<Vec<(&PathBuf, NaiveDateTime)>>();
        log_times.sort_by_key(|(_log_file, time)| time.clone());

        log_times.iter().skip(50).for_each(|(log_file, _time)| {
            std::fs::remove_file(log_file).expect("could not delete old log file");
        });
    }
}
