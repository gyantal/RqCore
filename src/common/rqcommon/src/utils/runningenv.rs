use std::{collections::HashMap, env, fs,};
use log;

pub type RqCoreConfig = HashMap<String, String>;
/// Returns the path to the sensitive configuration folder based on the current OS and user.
/// 
/// On Windows, uses USERDOMAIN to identify the machine.
/// On Linux/MacOS, uses LOGNAME to identify the user.
pub fn sensitive_config_folder_path() -> String {
    if env::consts::OS == "windows" {
        // On windows, use USERDOMAIN, instead of USERNAME, because USERNAME can be the same on multiple machines
        // (e.g. "gyantal" on both GYANTAL-PC and GYANTAL-LAPTOP)
        let userdomain = env::var("USERDOMAIN").expect("Failed to get USERDOMAIN environment variable");
        match userdomain.as_str() {
            "GYANTAL-PC" => "h:/.shortcut-targets-by-id/0BzxkV1ug5ZxvVmtic1FsNTM5bHM/GDriveHedgeQuant/shared/GitHubRepos/NonCommitedSensitiveData/RqCore/".to_string(),
            "GYANTAL-LAPTOP" => "h:/.shortcut-targets-by-id/0BzxkV1ug5ZxvVmtic1FsNTM5bHM/GDriveHedgeQuant/shared/GitHubRepos/NonCommitedSensitiveData/RqCore/".to_string(),
            "BALAZS-PC" => "h:/.shortcut-targets-by-id/0BzxkV1ug5ZxvVmtic1FsNTM5bHM/GDriveHedgeQuant/shared/GitHubRepos/NonCommitedSensitiveData/RqCore/".to_string(),
            "BALAZS-LAPTOP" => "g:/.shortcut-targets-by-id/0BzxkV1ug5ZxvVmtic1FsNTM5bHM/GDriveHedgeQuant/shared/GitHubRepos/NonCommitedSensitiveData/RqCore/".to_string(),
            "DAYA-DESKTOP" => "g:/.shortcut-targets-by-id/0BzxkV1ug5ZxvVmtic1FsNTM5bHM/GDriveHedgeQuant/shared/GitHubRepos/NonCommitedSensitiveData/RqCore/".to_string(),
            "DAYA-LAPTOP" => "g:/.shortcut-targets-by-id/0BzxkV1ug5ZxvVmtic1FsNTM5bHM/GDriveHedgeQuant/shared/GitHubRepos/NonCommitedSensitiveData/RqCore/".to_string(),
            "DRCHARMAT-LAPTOP" => "c:/Agy/NonCommitedSensitiveData/RqCore/".to_string(),
            _ => panic!("Windows user name is not recognized. Add your username and folder here!"),
        }
    } else {
        // Linux and MacOS
        // when running in "screen -r" session, LOGNAME is set, but USER is not
        let username = env::var("LOGNAME").expect("Failed to get LOGNAME environment variable");
        format!("/home/{}/RQ/sensitive_data/", username) // e.g. "/home/rquser/RQ/sensitive_data/https_certs"
    }
}

pub fn load_rqcore_config() -> Result<RqCoreConfig, String> {
    let sensitive_config_folder_path = sensitive_config_folder_path();
    let rqcore_config_path = format!("{}rqcore.config", sensitive_config_folder_path);

    let content = match fs::read_to_string(&rqcore_config_path) {
    Ok(content) => content,
    Err(err) => {
        log::error!("Failed to read config file '{}': {}", rqcore_config_path, err);
        return Err("Configuration file missing or unreadable".into());
        }
    };

    let mut rqconfig = HashMap::new();

    for (line_no, line) in content.lines().enumerate() {
        let line = line.trim();

        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let (key, value) = line.split_once('=')
            .ok_or_else(|| format!("Invalid config format at line {}", line_no + 1))?;

        rqconfig.insert(key.trim().to_string(), value.trim().to_string(),);
    }

    Ok(rqconfig)
}