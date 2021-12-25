use anyhow::Result;
use log::{debug, trace, LevelFilter};
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, thread};
use structopt::StructOpt;

#[derive(StructOpt, Debug)]
#[structopt()]
struct CliArgs {
    #[structopt(short, long, parse(from_os_str))]
    config_file: PathBuf,

    #[structopt(short, long, default_value = "info")]
    log_level: LevelFilter,
}

#[derive(Deserialize, Serialize, Debug)]
pub struct GitRepository {
    local_path: PathBuf,
    remote: String,
    fetch_branches: Vec<String>,
}

#[derive(Deserialize, Serialize, Debug)]
pub struct Config {
    repositories: Vec<GitRepository>,
}

fn init_logging(log_level: LevelFilter) -> Result<()> {
    use log4rs::{
        append::console::ConsoleAppender,
        config::{Appender, Config, Root},
        Handle,
    };
    let stdout = ConsoleAppender::builder().build();
    let config = Config::builder()
        .appender(Appender::builder().build("stdout", Box::new(stdout)))
        .build(Root::builder().appender("stdout").build(log_level));
    let _: Handle = log4rs::init_config(config?)?;
    Ok(())
}

fn load_config(config_file: PathBuf) -> Result<Config> {
    let mut settings = config::Config::default();
    settings.merge(config::File::from(config_file))?;
    let config = settings.try_into()?;
    Ok(config)
}

fn handle_repository(repository: GitRepository) {
    let GitRepository {
        local_path,
        remote,
        fetch_branches,
    } = repository;
    let repository = git2::Repository::open(local_path).unwrap();
    let remote = repository.find_remote(&remote);
    let result = match remote {
        Ok(mut remote) => remote.fetch(&fetch_branches, None, None),
        Err(error) => Err(error),
    };
    result.unwrap()
}

fn main() {
    let CliArgs {
        config_file,
        log_level,
    } = CliArgs::from_args();
    let logger_init_result = init_logging(log_level);
    trace!("Initialized logger {:?}", logger_init_result);
    let config = load_config(config_file).unwrap();
    debug!("Loaded config {:?}", config);
    let Config { repositories } = config;
    let mut handles = Vec::new();
    for repository in repositories {
        handles.push(thread::spawn(move || handle_repository(repository)));
    }
    handles.into_iter().for_each(|cur_thread| {
        cur_thread.join().unwrap();
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_cmd::Command;
    use assert_fs::prelude::*;

    fn command() -> assert_cmd::Command {
        Command::cargo_bin("git-auto-fetch").unwrap()
    }

    #[test]
    fn test_logging() {
        let mut cmd = command();
        let config = serde_json::to_string(&Config {
            repositories: Vec::new(),
        })
        .unwrap();
        let temp = assert_fs::TempDir::new().unwrap();
        let config_file = temp.child("config.json");
        config_file.write_str(&config).unwrap();
        let assert = cmd.arg("--config-file").arg(config_file.path()).assert();
        assert.code(0);
    }

    #[test]
    fn test_fetch() {
        let mut cmd = command();
        let temp = assert_fs::TempDir::new().unwrap();
        let remote_name = "origin";
        let local_dir = temp.child("local");
        let remote_dir = temp.child("remote");
        let remote_repository = git2::Repository::init(remote_dir.to_path_buf()).unwrap();
        let local_repository = git2::Repository::init(local_dir.to_path_buf()).unwrap();
        local_repository.remote(
            remote_name,
            &format!("file://{}/.git", remote_dir.to_path_buf().to_str().unwrap()),
        );
        let config = serde_json::to_string(&Config {
            repositories: vec![GitRepository {
                local_path: local_dir.to_path_buf(),
                fetch_branches: vec!["main".to_string()],
                remote: "origin".to_string(),
            }],
        })
        .unwrap();
        let config_file = temp.child("config.json");
        config_file.write_str(&config).unwrap();
        let assert = cmd.arg("--config-file").arg(config_file.path()).assert();
        assert.code(0);
    }
}
