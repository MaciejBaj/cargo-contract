// Copyright 2018-2020 Parity Technologies (UK) Ltd.
// This file is part of cargo-contract.
//
// cargo-contract is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// cargo-contract is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with cargo-contract.  If not, see <http://www.gnu.org/licenses/>.

mod cmd;
mod crate_metadata;
mod util;
mod workspace;

#[cfg(feature = "extrinsics")]
use sp_core::{
    crypto::{AccountId32, Pair},
    sr25519, Public, H256,
};

use std::{
    convert::{TryFrom, TryInto},
    path::PathBuf,
};
#[cfg(feature = "extrinsics")]
use subxt::PairSigner;

use anyhow::{Error, Result};
use colored::Colorize;
use structopt::{clap, StructOpt};

use crate::crate_metadata::CrateMetadata;

#[derive(Debug, StructOpt)]
#[structopt(bin_name = "cargo")]
pub(crate) enum Opts {
    /// Utilities to develop Wasm smart contracts.
    #[structopt(name = "contract")]
    #[structopt(setting = clap::AppSettings::UnifiedHelpMessage)]
    #[structopt(setting = clap::AppSettings::DeriveDisplayOrder)]
    #[structopt(setting = clap::AppSettings::DontCollapseArgsInUsage)]
    Contract(ContractArgs),
}

#[derive(Debug, StructOpt)]
pub(crate) struct ContractArgs {
    #[structopt(subcommand)]
    cmd: Command,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct HexData(pub Vec<u8>);

#[cfg(feature = "extrinsics")]
impl std::str::FromStr for HexData {
    type Err = hex::FromHexError;

    fn from_str(input: &str) -> std::result::Result<Self, Self::Err> {
        hex::decode(input).map(HexData)
    }
}

/// Arguments required for creating and sending an extrinsic to a substrate node
#[cfg(feature = "extrinsics")]
#[derive(Debug, StructOpt)]
pub(crate) struct ExtrinsicOpts {
    /// Websockets url of a substrate node
    #[structopt(
        name = "url",
        long,
        parse(try_from_str),
        default_value = "ws://localhost:9944"
    )]
    url: url::Url,
    /// Secret key URI for the account deploying the contract.
    #[structopt(name = "suri", long, short)]
    suri: String,
    /// Password for the secret key
    #[structopt(name = "password", long, short)]
    password: Option<String>,
}

#[cfg(feature = "extrinsics")]
impl ExtrinsicOpts {
    pub fn signer(&self) -> Result<PairSigner<subxt::ContractsTemplateRuntime, sr25519::Pair>> {
        let pair =
            sr25519::Pair::from_string(&self.suri, self.password.as_ref().map(String::as_ref))
                .map_err(|_| anyhow::anyhow!("Secret string error"))?;
        Ok(PairSigner::new(pair))
    }
}

#[derive(Debug, StructOpt)]
struct VerbosityFlags {
    #[structopt(long)]
    quiet: bool,
    #[structopt(long)]
    verbose: bool,
}

#[derive(Clone, Copy)]
enum Verbosity {
    Quiet,
    Verbose,
}

impl TryFrom<&VerbosityFlags> for Option<Verbosity> {
    type Error = Error;

    fn try_from(value: &VerbosityFlags) -> Result<Self, Self::Error> {
        match (value.quiet, value.verbose) {
            (false, false) => Ok(None),
            (true, false) => Ok(Some(Verbosity::Quiet)),
            (false, true) => Ok(Some(Verbosity::Verbose)),
            (true, true) => anyhow::bail!("Cannot pass both --quiet and --verbose flags"),
        }
    }
}

#[derive(Debug, StructOpt)]
struct UnstableOptions {
    /// Use the original manifest (Cargo.toml), do not modify for build optimizations
    #[structopt(long = "unstable-options", short = "Z", number_of_values = 1)]
    options: Vec<String>,
}

#[derive(Clone, Default)]
struct UnstableFlags {
    original_manifest: bool,
}

impl TryFrom<&UnstableOptions> for UnstableFlags {
    type Error = Error;

    fn try_from(value: &UnstableOptions) -> Result<Self, Self::Error> {
        let valid_flags = ["original-manifest"];
        let invalid_flags = value
            .options
            .iter()
            .filter(|o| !valid_flags.contains(&o.as_str()))
            .collect::<Vec<_>>();
        if !invalid_flags.is_empty() {
            anyhow::bail!("Unknown unstable-options {:?}", invalid_flags)
        }
        Ok(UnstableFlags {
            original_manifest: value.options.contains(&"original-manifest".to_owned()),
        })
    }
}

#[derive(Debug, StructOpt)]
enum Command {
    /// Setup and create a new smart contract project
    #[structopt(name = "new")]
    New {
        /// The name of the newly created smart contract
        name: String,
        /// The optional target directory for the contract project
        #[structopt(short, long, parse(from_os_str))]
        target_dir: Option<PathBuf>,
    },
    /// Compiles the smart contract
    #[structopt(name = "build")]
    Build {
        #[structopt(flatten)]
        verbosity: VerbosityFlags,
        #[structopt(flatten)]
        unstable_options: UnstableOptions,
    },
    /// Compiles all of the composable smart contracts described in the schedule
    #[structopt(name = "composable-build")]
    ComposableBuild {
        #[structopt(flatten)]
        verbosity: VerbosityFlags,
        #[structopt(flatten)]
        unstable_options: UnstableOptions,
    },
    /// Generate contract metadata artifacts
    #[structopt(name = "generate-metadata")]
    GenerateMetadata {
        #[structopt(flatten)]
        verbosity: VerbosityFlags,
        #[structopt(flatten)]
        unstable_options: UnstableOptions,
    },
    /// Test the smart contract off-chain
    #[structopt(name = "test")]
    Test {},
    /// Upload the smart contract code to the chain
    #[cfg(feature = "extrinsics")]
    #[structopt(name = "deploy")]
    Deploy {
        #[structopt(flatten)]
        extrinsic_opts: ExtrinsicOpts,
        /// Path to wasm contract code, defaults to ./target/<name>-pruned.wasm
        #[structopt(parse(from_os_str))]
        wasm_path: Option<PathBuf>,
    },
    /// Upload all smart contracts selected in composable schedule to appointed by urls chains.
    #[cfg(feature = "extrinsics")]
    #[structopt(name = "composable-deploy")]
    ComposableDeploy {
        /// Secret key URI for the account deploying the contract.
        #[structopt(name = "suri", long, short)]
        suri: String,
    },
    /// Instantiate a deployed smart contract
    #[cfg(feature = "extrinsics")]
    #[structopt(name = "instantiate")]
    Instantiate {
        #[structopt(flatten)]
        extrinsic_opts: ExtrinsicOpts,
        /// Transfers an initial balance to the instantiated contract
        #[structopt(name = "endowment", long, default_value = "0")]
        endowment: u128,
        /// Maximum amount of gas to be used for this command
        #[structopt(name = "gas", long, default_value = "500000000")]
        gas_limit: u64,
        /// The hash of the smart contract code already uploaded to the chain
        #[structopt(long, parse(try_from_str = parse_code_hash))]
        code_hash: H256,
        /// Hex encoded data to call a contract constructor
        #[structopt(long)]
        data: HexData,
    },
    /// Call for smart contract execution on Runtime Gateway
    #[cfg(feature = "extrinsics")]
    #[structopt(name = "call-runtime-gateway")]
    CallRuntimeGateway {
        #[structopt(flatten)]
        extrinsic_opts: ExtrinsicOpts,
        /// Target chain destination
        #[structopt(name = "target", long, short)]
        target: String,
        /// Target chain destination
        #[structopt(name = "requester", long, short)]
        requester: String,
        /// Execution Phase
        #[structopt(name = "phase", long, default_value = "0")]
        phase: u8,
        /// Value of balance transfer optionally attached to the execution order
        #[structopt(name = "value", long, default_value = "0")]
        value: u128,
        /// Maximum amount of gas to be used for this command
        #[structopt(name = "gas", long, default_value = "500000000")]
        gas_limit: u64,
        /// Path to wasm contract code, defaults to ./target/<name>-pruned.wasm
        #[structopt(parse(from_os_str))]
        wasm_path: Option<PathBuf>,
        /// Hex encoded data to call a contract constructor
        #[structopt(long, default_value = "00")]
        data: HexData,
    },
    /// Call for smart contract execution on Runtime Gateway
    #[cfg(feature = "extrinsics")]
    #[structopt(name = "call-contracts-gateway")]
    CallContractsGateway {
        #[structopt(flatten)]
        extrinsic_opts: ExtrinsicOpts,
        /// Target chain destination
        #[structopt(long, default_value = "00")]
        target: HexData,
        /// Target chain destination
        #[structopt(name = "requester", long, short)]
        requester: String,
        /// Execution Phase
        #[structopt(name = "phase", long, default_value = "0")]
        phase: u8,
        /// Value of balance transfer optionally attached to the execution order
        #[structopt(name = "value", long, default_value = "0")]
        value: u128,
        /// Maximum amount of gas to be used for this command
        #[structopt(name = "gas", long, default_value = "3875000000")]
        gas_limit: u64,
        /// Path to wasm contract code, defaults to ./target/<name>-pruned.wasm
        #[structopt(parse(from_os_str))]
        wasm_path: Option<PathBuf>,
        /// Hex encoded data to call a contract constructor
        #[structopt(long, default_value = "00")]
        data: HexData,
    },
    /// Call a regular smart contract execution via Contracts Pallet Call
    #[cfg(feature = "extrinsics")]
    #[structopt(name = "call-contract")]
    CallContract {
        #[structopt(flatten)]
        extrinsic_opts: ExtrinsicOpts,
        /// Target chain destination
        #[structopt(long, default_value = "00")]
        target: HexData,
        /// Value of balance transfer optionally attached to the execution order
        #[structopt(name = "value", long, default_value = "0")]
        value: u128,
        /// Maximum amount of gas to be used for this command
        #[structopt(name = "gas", long, default_value = "3875000000")]
        gas_limit: u64,
        /// Hex encoded data to call a contract constructor
        #[structopt(long, default_value = "00")]
        data: HexData,
    },
}

#[cfg(feature = "extrinsics")]
fn parse_code_hash(input: &str) -> Result<H256> {
    let bytes = hex::decode(input)?;
    if bytes.len() != 32 {
        anyhow::bail!("Code hash should be 32 bytes in length")
    }
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&bytes);
    Ok(H256(arr))
}

fn main() {
    env_logger::init();

    let Opts::Contract(args) = Opts::from_args();
    match exec(args.cmd) {
        Ok(msg) => println!("\t{}", msg),
        Err(err) => eprintln!(
            "{} {}",
            "ERROR:".bright_red().bold(),
            format!("{:?}", err).bright_red()
        ),
    }
}

fn exec(cmd: Command) -> Result<String> {
    match &cmd {
        Command::New { name, target_dir } => cmd::new::execute(name, target_dir.as_ref()),
        Command::Build {
            verbosity,
            unstable_options,
        } => {
            let manifest_path = Default::default();
            let dest_wasm = cmd::build::execute(
                &manifest_path,
                verbosity.try_into()?,
                unstable_options.try_into()?,
            )?;
            Ok(format!(
                "\nYour contract is ready. You can find it here:\n{}",
                dest_wasm.display().to_string().bold()
            ))
        }
        Command::ComposableBuild {
            verbosity,
            unstable_options,
        } => {
            let manifest_path = Default::default();
            let dest_wasm = cmd::composable_build::execute(
                &manifest_path,
                verbosity.try_into()?,
                unstable_options.try_into()?,
            )?;
            Ok(format!(
                "\nYour composable contract(s) is/are ready. You can find it the following directory:\n{}",
                dest_wasm.display().to_string().bold()
            ))
        }
        Command::GenerateMetadata {
            verbosity,
            unstable_options,
        } => {
            let metadata_file = cmd::metadata::execute(
                Default::default(),
                verbosity.try_into()?,
                unstable_options.try_into()?,
            )?;
            Ok(format!(
                "Your metadata file is ready.\nYou can find it here:\n{}",
                metadata_file.display()
            ))
        }
        Command::Test {} => Err(anyhow::anyhow!("Command unimplemented")),
        #[cfg(feature = "extrinsics")]
        Command::Deploy {
            extrinsic_opts,
            wasm_path,
        } => {
            let code_hash = cmd::execute_deploy(extrinsic_opts, wasm_path.as_ref())?;
            Ok(format!("Code hash: {:?}", code_hash))
        }
        #[cfg(feature = "extrinsics")]
        Command::ComposableDeploy { suri } => {
            let manifest_path = Default::default();
            let crate_metadata = CrateMetadata::collect(&manifest_path)?;
            println!(
                "{}",
                "Deploy composable components to appointed urls"
                    .bright_blue()
                    .bold(),
            );
            let composable_schedule = crate_metadata.clone().t3rn_composable_schedule
                .expect("Failed to read composable metadata from JSON using serde. Make sure your Cargo.toml follows the composable metadata format");
            match composable_schedule.deploy {
                Some(deploy_schedule) => {
                    for deploy in deploy_schedule {
                        println!("Deploying: {:?}", deploy);
                        let component_extrinsic_opts = ExtrinsicOpts {
                            url: url::Url::parse(&deploy.url)?,
                            suri: suri.to_string(),
                            password: None,
                        };
                        let dest_wasm_path = cmd::composable_build::get_dest_wasm_path(
                            deploy.compose.clone(),
                            &crate_metadata.clone(),
                        );
                        let code_hash =
                            cmd::execute_deploy(&component_extrinsic_opts, Some(&dest_wasm_path))?;
                        println!(
                            "{} - {} {:?}",
                            deploy.compose.bright_blue().bold(),
                            "successfully deployed byte code with hash: ".bright_blue(),
                            code_hash
                        );
                    }
                    Ok(format!(
                        "All components successfully deployed for {:?}",
                        suri
                    ))
                }
                None => Err(anyhow::anyhow!(
                    "Nothing to deploy. Empty deploy key of composable metadata."
                )),
            }
        }
        #[cfg(feature = "extrinsics")]
        Command::Instantiate {
            extrinsic_opts,
            endowment,
            code_hash,
            gas_limit,
            data,
        } => {
            let contract_account = cmd::execute_instantiate(
                extrinsic_opts,
                *endowment,
                *gas_limit,
                *code_hash,
                data.clone(),
            )?;
            Ok(format!("Contract account: {:?}", contract_account))
        }
        #[cfg(feature = "extrinsics")]
        Command::CallRuntimeGateway {
            extrinsic_opts,
            target,
            requester,
            wasm_path,
            phase,
            value,
            gas_limit,
            data,
        } => {
            let code = cmd::deploy::load_contract_code(wasm_path.as_ref())?;

            let pair_target = sr25519::Pair::from_string(target, None)
                .map_err(|_| anyhow::anyhow!("Target account read string error"))?;

            let pair_requester = sr25519::Pair::from_string(requester, None)
                .map_err(|_| anyhow::anyhow!("Requester account read string error"))?;

            let res = cmd::execute_call(
                extrinsic_opts,
                AccountId32::from(pair_requester.public()),
                AccountId32::from(pair_target.public()),
                *phase,
                &code,
                *value,
                *gas_limit,
                data.clone(),
            )?;

            Ok(format!("CallRuntimeGateway result: {:?}", res))
        }
        #[cfg(feature = "extrinsics")]
        Command::CallContractsGateway {
            extrinsic_opts,
            target,
            requester,
            wasm_path,
            phase,
            value,
            gas_limit,
            data,
        } => {
            let code = match cmd::deploy::load_contract_code(wasm_path.as_ref()) {
                Ok(loaded_code) => loaded_code,
                Err(_) => {
                    println!(
                        "Correct code not found. Proceeding with a direct contract call at target_dest"
                    );
                    vec![]
                }
            };
            let pair_requester = sr25519::Pair::from_string(requester, None)
                .map_err(|_| anyhow::anyhow!("Requester account read string error"))?;
            println!(
                ".clone().0.as_slice() {:?} {:?} ",
                target,
                target.clone().0.as_slice()
            );
            let res = cmd::execute_contract_call(
                extrinsic_opts,
                AccountId32::from(pair_requester.public()),
                AccountId32::from(sr25519::Public::from_slice(target.0.as_slice())),
                *phase,
                &code,
                *value,
                *gas_limit,
                data.clone(),
            )?;

            Ok(format!("CallRuntimeGateway result: {:?}", res))
        }
        #[cfg(feature = "extrinsics")]
        Command::CallContract {
            extrinsic_opts,
            target,
            value,
            gas_limit,
            data,
        } => {
            let res = cmd::call_regular_contract(
                extrinsic_opts,
                AccountId32::from(sr25519::Public::from_slice(target.0.as_slice())),
                *value,
                *gas_limit,
                data.clone(),
            )?;

            Ok(format!("Call regular contract result: {:?}", res))
        }
    }
}
