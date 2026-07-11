use std::{ffi::OsString, path::PathBuf};

use yar_compiler::manifest::MANIFEST_FILE;

pub const ROOT_USAGE: &str =
    "usage: yar [--manifest-path <path/to/yar.toml>] <command> [arguments]";

const COMMANDS: &[CommandInfo] = &[
    CommandInfo {
        name: "check",
        usage: "usage: yar check <file|dir>",
        description: "Parse and type-check without generating LLVM IR or a binary",
        options: &[],
    },
    CommandInfo {
        name: "emit-ir",
        usage: "usage: yar emit-ir <file|dir>",
        description: "Print LLVM IR to stdout",
        options: &[],
    },
    CommandInfo {
        name: "build",
        usage: "usage: yar build <file|dir> [-o <output>]",
        description: "Compile to a native executable",
        options: &["  -o <output>  Write the executable to this path"],
    },
    CommandInfo {
        name: "run",
        usage: "usage: yar run <file|dir> [-- <argument>...]",
        description: "Compile and execute a temporary native executable",
        options: &[],
    },
    CommandInfo {
        name: "test",
        usage: "usage: yar test <file|dir>",
        description: "Discover and run test functions from _test.yar files",
        options: &[],
    },
    CommandInfo {
        name: "init",
        usage: "usage: yar init",
        description: "Create a yar.toml manifest",
        options: &[],
    },
    CommandInfo {
        name: "add",
        usage: "usage: yar add <alias> <git-url> (--tag=<tag>|--rev=<revision>|--branch=<branch>)\n       yar add <alias> --path=<dir>",
        description: "Add a dependency to yar.toml",
        options: &[
            "  --tag=<tag>          Select a Git tag",
            "  --rev=<revision>     Select a Git revision",
            "  --branch=<branch>    Select a Git branch",
            "  --path=<dir>         Select a local dependency",
        ],
    },
    CommandInfo {
        name: "remove",
        usage: "usage: yar remove <alias>",
        description: "Remove a dependency from yar.toml",
        options: &[],
    },
    CommandInfo {
        name: "fetch",
        usage: "usage: yar fetch",
        description: "Download dependencies from yar.lock to the cache",
        options: &[],
    },
    CommandInfo {
        name: "lock",
        usage: "usage: yar lock",
        description: "Regenerate yar.lock from yar.toml",
        options: &[],
    },
    CommandInfo {
        name: "update",
        usage: "usage: yar update [alias]",
        description: "Re-resolve dependencies and update yar.lock",
        options: &[],
    },
];

struct CommandInfo {
    name: &'static str,
    usage: &'static str,
    description: &'static str,
    options: &'static [&'static str],
}

pub enum Invocation {
    Help(String),
    Version,
    Command {
        command: String,
        command_args: Vec<OsString>,
        manifest_path: Option<PathBuf>,
    },
}

pub fn parse(mut args: Vec<OsString>) -> Result<Invocation, String> {
    let mut manifest_path = None;
    while args.first().is_some_and(|arg| arg == "--manifest-path") {
        if manifest_path.is_some() {
            return Err("--manifest-path may be specified only once".to_string());
        }
        if args.len() < 2 {
            return Err(ROOT_USAGE.to_string());
        }
        let path = PathBuf::from(&args[1]);
        if path.file_name() != Some(MANIFEST_FILE.as_ref()) {
            return Err("--manifest-path must name yar.toml".to_string());
        }
        manifest_path = Some(path);
        args.drain(..2);
    }

    if args.len() == 1 && is_help(&args[0]) {
        return Ok(Invocation::Help(root_help()));
    }
    if args.len() == 1 && is_version(&args[0]) {
        return Ok(Invocation::Version);
    }

    let Some(command) = args.first().and_then(|arg| arg.to_str()) else {
        return Err(ROOT_USAGE.to_string());
    };
    let Some(command_info) = COMMANDS.iter().find(|candidate| candidate.name == command) else {
        return Err(format!("unknown command {command:?}"));
    };
    if args.len() == 2 && is_help(&args[1]) {
        return Ok(Invocation::Help(command_help(command_info)));
    }

    Ok(Invocation::Command {
        command: command.to_owned(),
        command_args: args[1..].to_vec(),
        manifest_path,
    })
}

pub fn version() -> &'static str {
    option_env!("YAR_BUILD_VERSION").unwrap_or(env!("CARGO_PKG_VERSION"))
}

fn is_help(arg: &OsString) -> bool {
    arg == "-h" || arg == "--help"
}

fn is_version(arg: &OsString) -> bool {
    arg == "-V" || arg == "--version"
}

fn root_help() -> String {
    let mut help = format!("{ROOT_USAGE}\n\ncommands:\n");
    for command in COMMANDS {
        help.push_str(&format!("  {:<10}{}\n", command.name, command.description));
    }
    help.push_str(
        "\noptions:\n  --manifest-path <path/to/yar.toml>  Select a project manifest explicitly\n  -h, --help                          Print help\n  -V, --version                       Print version\n",
    );
    help
}

fn command_help(command: &CommandInfo) -> String {
    let mut help = format!("{}\n\n{}\n\noptions:\n", command.usage, command.description);
    for option in command.options {
        help.push_str(option);
        help.push('\n');
    }
    help.push_str("  -h, --help  Print help\n");
    help
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn program_arguments_are_not_interpreted_as_help() {
        let invocation = parse(
            ["run", "main.yar", "--", "--help"]
                .into_iter()
                .map(OsString::from)
                .collect(),
        )
        .unwrap();

        let Invocation::Command { command_args, .. } = invocation else {
            panic!("expected command invocation");
        };
        assert_eq!(
            command_args,
            ["main.yar", "--", "--help"].map(OsString::from)
        );
    }
}
