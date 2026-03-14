use arrayvec::{ArrayString, ArrayVec};
use dpedal_config::{
    ComputerInput, Config, DpedalInput, KeyboardInput, MAX_COMPUTER_INPUTS, MAX_DPEDAL_INPUTS,
    MAX_MAPPINGS, MAX_NICKNAME_LEN, MAX_PIN_REMAPPINGS, MAX_PROFILES, MappingMode, MouseInput,
};
use kdl::{KdlDocument, KdlNode};
use kdl_config::{
    KdlConfig, KdlConfigFinalize, Parsed,
    error::{ParseDiagnostic, ParseError},
};
use kdl_config_derive::{KdlConfig, KdlConfigFinalize};
use miette::{IntoDiagnostic, NamedSource, miette};
use rkyv::rancor::Error;
use std::{path::PathBuf, str::FromStr};

pub fn encode_config(config: &Config) -> miette::Result<Vec<u8>> {
    let bytes = rkyv::to_bytes::<Error>(config).map_err(|e| miette!(e))?;
    let mut result = vec![];
    result.extend((bytes.len() as u32).to_be_bytes());
    result.extend(bytes.iter());
    Ok(result)
}

pub fn load(path: Option<PathBuf>) -> miette::Result<Config> {
    let input = load_source(path)?;
    // TODO: upstream a way to tell KDL parser what the filename is.
    let kdl: KdlDocument = input.inner().parse()?;
    let (profile, error): (Parsed<ConfigKdl>, ParseError) = kdl_config::parse(input, kdl);

    // TODO: extra diagnostics here.

    if !error.diagnostics.is_empty() {
        return Err(error.into());
    }

    Ok(profile.value.finalize())
}

fn load_source(path: Option<PathBuf>) -> miette::Result<NamedSource<String>> {
    let path = if let Some(path) = path {
        path
    } else if let Ok(cargo_manifest_dir) = std::env::var("CARGO_MANIFEST_DIR") {
        PathBuf::from(cargo_manifest_dir)
            .parent()
            .unwrap()
            .join("config.kdl")
    } else {
        std::env::current_exe()
            .unwrap()
            .parent()
            .unwrap()
            .join("config.kdl")
    };
    let filename = path.file_name().unwrap().to_str().unwrap();
    let text = std::fs::read_to_string(&path)
        .into_diagnostic()
        .map_err(|e| e.context(format!("Failed to load config file at {path:?}")))?;
    Ok(NamedSource::new(filename, text))
}

#[derive(KdlConfig, KdlConfigFinalize, Default, Debug)]
#[kdl_config_finalize_into = "dpedal_config::Config"]
pub struct ConfigKdl {
    pub version: Parsed<u32>,
    pub nickname: Parsed<ArrayString<MAX_NICKNAME_LEN>>,
    pub device: Parsed<DeviceKdl>,
    pub color: Parsed<u32>,
    pub profiles: Parsed<ArrayVec<Parsed<ProfileKdl>, MAX_PROFILES>>,
    // TODO: add validation: no duplicate pins (including default values), valid pin range
    pub pin_remappings: Parsed<ArrayVec<Parsed<PinRemappingKdl>, MAX_PIN_REMAPPINGS>>,
}

#[derive(KdlConfig, KdlConfigFinalize, Default, Debug)]
#[kdl_config_finalize_into = "dpedal_config::PinRemapping"]
pub struct PinRemappingKdl {
    pub input: Parsed<DpedalInputKdl>,
    pub pin: Parsed<u32>,
}

// TODO: add derive side validation that Parsed is used everywhere.
#[derive(KdlConfig, KdlConfigFinalize, Default, Debug)]
#[kdl_config_finalize_into = "dpedal_config::Profile"]
pub struct ProfileKdl {
    pub mappings: Parsed<ArrayVec<Parsed<MappingKdl>, MAX_MAPPINGS>>,
}

#[derive(Default, Debug)]
pub struct MappingKdl {
    pub input_set: ArrayVec<dpedal_config::DpedalInput, MAX_DPEDAL_INPUTS>,
    pub mode: MappingMode,
    pub output_sequence: ArrayVec<dpedal_config::ComputerInput, MAX_COMPUTER_INPUTS>,
}

impl KdlConfigFinalize for MappingKdl {
    type FinalizeType = dpedal_config::Mapping;

    fn finalize(&self) -> Self::FinalizeType {
        Self::FinalizeType {
            input_set: self.input_set.clone(),
            mode: self.mode,
            output_sequence: self.output_sequence.clone(),
        }
    }
}

impl KdlConfig for MappingKdl {
    fn parse_as_node(
        source: NamedSource<String>,
        node: &KdlNode,
        diagnostics: &mut Vec<kdl_config::error::ParseDiagnostic>,
    ) -> Parsed<Self>
    where
        Self: Sized,
    {
        let Some(entry) = node.entries().first() else {
            diagnostics.push(ParseDiagnostic {
                input: source.clone(),
                span: node.span(),
                message: Some("Unexpected format".to_owned()),
                label: None,
                help: None,
                severity: miette::Severity::Error,
            });
            return Parsed {
                value: Default::default(),
                full_span: node.span(),
                name_span: node.span(),
                valid: false,
            };
        };
        match entry.value() {
            kdl::KdlValue::String(value) => {
                let mut split = value.split("->");
                let input = split.next().unwrap().trim();
                let Some(output) = split.next() else {
                    diagnostics.push(ParseDiagnostic {
                        input: source.clone(),
                        span: node.span(),
                        message: Some(
                            "Mapping needs to follow format `input -> output` but `->` was not present".to_owned()
                        ),
                        label: None,
                        help: None,
                        severity: miette::Severity::Error,
                    });
                    return Parsed {
                        value: Default::default(),
                        full_span: node.span(),
                        name_span: node.span(),
                        valid: false,
                    };
                };
                let output = output.trim();

                let Some(input) = DpedalInput::from_string_kebab(input) else {
                    diagnostics.push(ParseDiagnostic {
                        input: source.clone(),
                        span: node.span(),
                        message: Some(format!("Unknown input {input:?}")),
                        label: None,
                        help: None,
                        severity: miette::Severity::Error,
                    });
                    return Parsed {
                        value: Default::default(),
                        full_span: node.span(),
                        name_span: node.span(),
                        valid: false,
                    };
                };
                let input = ArrayVec::from_iter([input]);

                let Some((ty, sub_ty)) = output.split_once("-") else {
                    diagnostics.push(ParseDiagnostic {
                        input: source.clone(),
                        span: node.span(),
                        message: Some(format!("Unknown output {output:?}")),
                        label: None,
                        help: None,
                        severity: miette::Severity::Error,
                    });
                    return Parsed {
                        value: Default::default(),
                        full_span: node.span(),
                        name_span: node.span(),
                        valid: false,
                    };
                };

                let output = match ty {
                    "mouse" => match MouseInput::from_string(sub_ty, "20") {
                        Some(input) => ComputerInput::Mouse(input),
                        None => {
                            diagnostics.push(ParseDiagnostic {
                                input: source.clone(),
                                span: node.span(),
                                message: Some(format!("Unknown output {output:?}")),
                                label: None,
                                help: None,
                                severity: miette::Severity::Error,
                            });
                            return Parsed {
                                value: Default::default(),
                                full_span: node.span(),
                                name_span: node.span(),
                                valid: false,
                            };
                        }
                    },
                    "keyboard" => match keyboard_from_string_kebab(sub_ty) {
                        Some(input) => ComputerInput::Keyboard(input),
                        None => {
                            diagnostics.push(ParseDiagnostic {
                                input: source.clone(),
                                span: node.span(),
                                message: Some(format!("Unknown output {output:?}")),
                                label: None,
                                help: None,
                                severity: miette::Severity::Error,
                            });
                            return Parsed {
                                value: Default::default(),
                                full_span: node.span(),
                                name_span: node.span(),
                                valid: false,
                            };
                        }
                    },
                    _ => {
                        diagnostics.push(ParseDiagnostic {
                            input: source.clone(),
                            span: node.span(),
                            message: Some(format!("Unknown output {output:?}")),
                            label: None,
                            help: None,
                            severity: miette::Severity::Error,
                        });
                        return Parsed {
                            value: Default::default(),
                            full_span: node.span(),
                            name_span: node.span(),
                            valid: false,
                        };
                    }
                };
                let output_sequence = ArrayVec::from_iter([output]);
                Parsed {
                    value: MappingKdl {
                        input_set: input,
                        mode: MappingMode::OnPressUntilRelease,
                        output_sequence,
                    },
                    full_span: node.span(),
                    name_span: node.span(),
                    valid: true,
                }
            }
            value => {
                diagnostics.push(ParseDiagnostic {
                    input: source.clone(),
                    span: node.span(),
                    message: Some(format!(
                        "Node contains {value:?} but expected it to contain a string"
                    )),
                    label: None,
                    help: None,
                    severity: miette::Severity::Error,
                });
                Parsed {
                    value: Default::default(),
                    full_span: node.span(),
                    name_span: node.span(),
                    valid: false,
                }
            }
        }
    }
}

pub fn keyboard_from_string_kebab(s: &str) -> Option<KeyboardInput> {
    let mut pascal_case = String::new();

    let mut upper = true;
    for char in s.chars() {
        if upper {
            pascal_case.push(char.to_ascii_uppercase());
            upper = false;
        } else if char == '-' {
            upper = true;
        } else {
            pascal_case.push(char);
        }
    }
    KeyboardInput::from_str(&pascal_case).ok()
}

#[test]
fn test_keyboard_from_string_kebab() {
    assert_eq!(
        keyboard_from_string_kebab("page-up").unwrap(),
        KeyboardInput::PageUp
    );
    assert_eq!(keyboard_from_string_kebab("a").unwrap(), KeyboardInput::A);
}

#[derive(KdlConfig, KdlConfigFinalize, Default, Debug)]
#[kdl_config_finalize_into = "dpedal_config::DpedalInput"]
pub enum DpedalInputKdl {
    #[default]
    DpadUp,
    DpadDown,
    DpadLeft,
    DpadRight,
    ButtonLeft,
    ButtonRight,
}

#[derive(KdlConfig, KdlConfigFinalize, Default, Debug)]
#[kdl_config_finalize_into = "dpedal_config::Device"]
pub enum DeviceKdl {
    #[default]
    DpedalV3,
}
