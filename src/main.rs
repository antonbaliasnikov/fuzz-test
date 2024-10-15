#[macro_use]
extern crate afl;
extern crate era_compiler_solidity;

use era_compiler_solidity::{
    CollectableError, Project, SolcCompiler, SolcPipeline, SolcStandardJsonInput,
    SolcStandardJsonInputSettingsOptimizer, SolcStandardJsonInputSettingsSelection,
    SolcStandardJsonOutput,
};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::process::Command;
use std::str::FromStr;

pub const MAIN_CODE: &str = r#"
// SPDX-License-Identifier: MIT

pragma solidity >=0.4.12;

contract Test {
    uint256 value;

    function set(uint256 x) external {
        value = x;
    }

    function get() external view returns(uint256) {
        return value;
    }
}
"#;

pub fn get_solc_compiler(solc_version: &semver::Version) -> anyhow::Result<SolcCompiler> {
    let solc_path = PathBuf::from("solc-bin").join(format!(
        "{}-{}{}",
        SolcCompiler::DEFAULT_EXECUTABLE_NAME,
        solc_version,
        std::env::consts::EXE_SUFFIX,
    ));

    SolcCompiler::new(solc_path.to_str().unwrap())
}

pub fn build_solidity(
    sources: BTreeMap<String, String>,
    libraries: BTreeMap<String, BTreeMap<String, String>>,
    remappings: Option<BTreeSet<String>>,
    solc_version: &semver::Version,
    solc_pipeline: SolcPipeline,
    optimizer_settings: era_compiler_llvm_context::OptimizerSettings,
) -> anyhow::Result<SolcStandardJsonOutput> {
    // Set the `zksolc` binary path
    let zksolc_bin = Command::new(era_compiler_solidity::DEFAULT_EXECUTABLE_NAME);
    let _ = era_compiler_solidity::process::EXECUTABLE.set(PathBuf::from(zksolc_bin.get_program()));

    // Enable LLVM pretty stack trace
    inkwell::support::enable_llvm_pretty_stack_trace();

    let solc_compiler = get_solc_compiler(solc_version)?;

    era_compiler_llvm_context::initialize_target(era_compiler_common::Target::EraVM);

    let mut solc_input = SolcStandardJsonInput::try_from_solidity_sources(
        None,
        sources.clone(),
        libraries.clone(),
        remappings,
        SolcStandardJsonInputSettingsSelection::new_required(Some(solc_pipeline)),
        SolcStandardJsonInputSettingsOptimizer::new(
            true,
            None,
            &solc_compiler.version.default,
            false,
        ),
        None,
        solc_pipeline == SolcPipeline::EVMLA,
        false,
        true,
        false,
        vec![],
        vec![],
        vec![],
    )?;

    let mut solc_output = solc_compiler.standard_json(
        &mut solc_input,
        Some(solc_pipeline),
        &mut vec![],
        None,
        vec![],
        None,
    )?;
    solc_output.take_and_write_warnings();
    solc_output.collect_errors()?;

    let project = Project::try_from_solc_output(
        libraries,
        solc_pipeline,
        &mut solc_output,
        &solc_compiler,
        None,
    )?;
    solc_output.take_and_write_warnings();
    solc_output.collect_errors()?;

    let build = project.compile_to_eravm(
        &mut vec![],
        true,
        era_compiler_common::HashType::Ipfs,
        optimizer_settings,
        vec![],
        false,
        None,
        None,
    )?;
    build.write_to_standard_json(
        &mut solc_output,
        Some(&solc_compiler.version),
        &semver::Version::from_str(env!("CARGO_PKG_VERSION"))?,
    )?;

    solc_output.take_and_write_warnings();
    solc_output.collect_errors()?;
    Ok(solc_output)
}

fn main() {
    fuzz!(|data: &[u8]| {
        let mut sources = BTreeMap::new();

        if let Ok(s) = std::str::from_utf8(data) {
            sources.insert("main.sol".to_owned(), s.to_owned());
        }

        build_solidity(
            sources.clone(),
            BTreeMap::new(),
            None,
            &semver::Version::new(0, 8, 27),
            SolcPipeline::EVMLA,
            era_compiler_llvm_context::OptimizerSettings::cycles(),
        )
        .expect("Panic");
    });
}
