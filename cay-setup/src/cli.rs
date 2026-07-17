use std::path::PathBuf;

use crate::{Error, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Install(InstallOptions),
    Update(InstallOptions),
    Uninstall { yes: bool, root: Option<PathBuf> },
    Show { root: Option<PathBuf> },
    Doctor { root: Option<PathBuf> },
    Version,
    Help,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InstallOptions {
    pub version: Option<String>,
    pub root: Option<PathBuf>,
    pub yes: bool,
    pub modify_path: bool,
}

pub fn parse<I, S>(args: I) -> Result<Command>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let mut args: Vec<String> = args.into_iter().map(Into::into).collect();
    if args.is_empty() {
        return Ok(Command::Install(default_install_options()));
    }

    let command = args.remove(0);
    match command.as_str() {
        "install" => Ok(Command::Install(parse_install_options(args)?)),
        "update" => Ok(Command::Update(parse_install_options(args)?)),
        "uninstall" => {
            let (yes, root) = parse_management_options(args, true)?;
            Ok(Command::Uninstall { yes, root })
        }
        "show" => {
            let (_, root) = parse_management_options(args, false)?;
            Ok(Command::Show { root })
        }
        "doctor" => {
            let (_, root) = parse_management_options(args, false)?;
            Ok(Command::Doctor { root })
        }
        "-V" | "--version" => no_extra_args(args, Command::Version),
        "-h" | "--help" | "help" => no_extra_args(args, Command::Help),
        unknown if unknown.starts_with('-') => {
            let mut all = vec![unknown.to_string()];
            all.extend(args);
            Ok(Command::Install(parse_install_options(all)?))
        }
        unknown => Err(Error::InvalidArgument(format!(
            "未知命令 `{unknown}`；运行 `cay-setup --help` 查看用法"
        ))),
    }
}

fn default_install_options() -> InstallOptions {
    InstallOptions {
        modify_path: true,
        ..InstallOptions::default()
    }
}

fn parse_install_options(args: Vec<String>) -> Result<InstallOptions> {
    let mut options = default_install_options();
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "-y" | "--yes" => options.yes = true,
            "--no-modify-path" => options.modify_path = false,
            "--version" => {
                index += 1;
                options.version = Some(required_value(&args, index, "--version")?);
            }
            "--root" => {
                index += 1;
                options.root = Some(PathBuf::from(required_value(&args, index, "--root")?));
            }
            arg => {
                return Err(Error::InvalidArgument(format!("未知选项 `{arg}`")));
            }
        }
        index += 1;
    }
    Ok(options)
}

fn required_value(args: &[String], index: usize, flag: &str) -> Result<String> {
    args.get(index)
        .filter(|value| !value.starts_with('-'))
        .cloned()
        .ok_or_else(|| Error::InvalidArgument(format!("`{flag}` 缺少值")))
}

fn parse_management_options(args: Vec<String>, allow_yes: bool) -> Result<(bool, Option<PathBuf>)> {
    let mut yes = false;
    let mut root = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "-y" | "--yes" if allow_yes => yes = true,
            "--root" => {
                index += 1;
                root = Some(PathBuf::from(required_value(&args, index, "--root")?));
            }
            arg => return Err(Error::InvalidArgument(format!("未知选项 `{arg}`"))),
        }
        index += 1;
    }
    Ok((yes, root))
}

fn no_extra_args(args: Vec<String>, command: Command) -> Result<Command> {
    if let Some(arg) = args.first() {
        Err(Error::InvalidArgument(format!("多余参数 `{arg}`")))
    } else {
        Ok(command)
    }
}

pub const HELP: &str = r#"Cavvy 工具链安装器

用法:
  cay-setup                         安装最新稳定版
  cay-setup install [选项]          安装 Cavvy
  cay-setup update [选项]           更新到最新稳定版
  cay-setup uninstall [-y] [--root] 卸载 Cavvy
  cay-setup show [--root <目录>]    显示当前安装
  cay-setup doctor [--root <目录>]  检查并试编译工具链

安装选项:
  --version <版本>                  安装指定版本，例如 6.1.0
  --root <目录>                     覆盖安装根目录
  --no-modify-path                  不修改用户 PATH
  -y, --yes                         跳过确认

环境变量:
  CAVVY_HOME                        默认安装根目录，默认 ~/.cavvy
  CAVVY_RELEASE_API                 Release API 地址，用于镜像或测试
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_arguments_installs_latest_and_modifies_path() {
        assert_eq!(
            parse(Vec::<String>::new()).unwrap(),
            Command::Install(InstallOptions {
                modify_path: true,
                ..InstallOptions::default()
            })
        );
    }

    #[test]
    fn parses_non_interactive_versioned_install() {
        assert_eq!(
            parse(["install", "--version", "6.1.0", "-y", "--no-modify-path"]).unwrap(),
            Command::Install(InstallOptions {
                version: Some("6.1.0".to_string()),
                yes: true,
                modify_path: false,
                root: None,
            })
        );
    }

    #[test]
    fn top_level_options_keep_one_command_bootstrap_compatible() {
        assert!(matches!(
            parse(["--yes", "--root", "C:/Cavvy"]).unwrap(),
            Command::Install(InstallOptions { yes: true, .. })
        ));
    }

    #[test]
    fn rejects_unknown_commands() {
        assert!(parse(["download"]).is_err());
    }

    #[test]
    fn management_commands_accept_the_same_custom_root() {
        assert_eq!(
            parse(["doctor", "--root", "D:/Cavvy"]).unwrap(),
            Command::Doctor {
                root: Some(PathBuf::from("D:/Cavvy"))
            }
        );
        assert_eq!(
            parse(["uninstall", "--root", "D:/Cavvy", "--yes"]).unwrap(),
            Command::Uninstall {
                yes: true,
                root: Some(PathBuf::from("D:/Cavvy"))
            }
        );
    }
}
