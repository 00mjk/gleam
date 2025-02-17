use gleam_core::{
    build::{Mode, Options, Target},
    config::PackageConfig,
    error::Error,
    paths,
};
use std::process::Command;

#[derive(Debug, Clone, Copy)]
pub enum Which {
    Src,
    Test,
}

pub fn command(arguments: &[String], which: Which) -> Result<(), Error> {
    let config = crate::config::root_config()?;

    // Determine which module to run
    let module = match which {
        Which::Src => config.name.to_string(),
        Which::Test => format!("{}_test", &config.name),
    };

    // Build project so we have bytecode to run
    let _ = crate::build::main(&Options {
        perform_codegen: true,
        mode: Mode::Dev,
        target: None,
    })?;

    // Don't exit on ctrl+c as it is used by child erlang shell
    ctrlc::set_handler(move || {}).expect("Error setting Ctrl-C handler");

    // Prepare the Erlang shell command
    let mut command = match config.target {
        Target::Erlang => erlang_command(&config, &module, arguments),
        Target::JavaScript => javascript_command(&config, &module, arguments),
    }?;

    crate::cli::print_running(&format!("{}.main", module));

    // Run the shell
    tracing::info!("Running OS process {:?}", command);
    let status = command.status().map_err(|e| Error::ShellCommand {
        command: "erl".to_string(),
        err: Some(e.kind()),
    })?;

    std::process::exit(status.code().unwrap_or_default());
}

fn erlang_command(
    config: &PackageConfig,
    module: &str,
    arguments: &[String],
) -> Result<Command, Error> {
    let mut command = Command::new("erl");

    // Specify locations of .beam files
    let packages = paths::build_packages(Mode::Dev, config.target);

    for entry in crate::fs::read_dir(&packages)?.filter_map(Result::ok) {
        let _ = command.arg("-pa").arg(entry.path().join("ebin"));
    }

    let _ = command.arg("-eval");
    let _ = command.arg(format!("gleam@@main:run({})", &module));

    // Don't run the Erlang shell
    let _ = command.arg("-noshell");

    // Tell the BEAM that any following argument are for the program
    let _ = command.arg("-extra");
    for argument in arguments {
        let _ = command.arg(argument);
    }
    Ok(command)
}

fn javascript_command(
    config: &PackageConfig,
    module: &str,
    arguments: &[String],
) -> Result<Command, Error> {
    let module = paths::build_package(Mode::Dev, Target::JavaScript, &config.name)
        .join("dist")
        .join(module);
    let mut command = Command::new("node");

    // Run the main function.
    let _ = command.arg("-e");
    let _ = command.arg(&format!(
        "import('./{}.mjs').then(module => module.main())",
        module.to_string_lossy()
    ));

    // Tell Node that any following argument are for the program
    let _ = command.arg("--");
    for argument in arguments {
        let _ = command.arg(argument);
    }

    Ok(command)
}
