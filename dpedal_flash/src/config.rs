use dpedal_config::{
    ComputerInput, Config, DpedalInput, KeyboardInput, MAX_COMPUTER_INPUTS, MAX_DPEDAL_INPUTS,
    MAX_MAPPINGS, MAX_NICKNAME_LEN, MAX_PIN_REMAPPINGS, MAX_PROFILES, MappingMode, Meta,
    MouseInput,
};
use kdl::{KdlDocument, KdlNode};
use kdl_config::{
    KdlConfig, KdlConfigFinalize, Parsed,
    error::{ParseDiagnostic, ParseError},
};
use kdl_config_derive::{KdlConfig, KdlConfigFinalize};
use miette::{IntoDiagnostic, NamedSource};
use std::{path::PathBuf, str::FromStr};

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

#[derive(KdlConfig, Default, Debug)]
pub struct ConfigKdl {
    pub version: Parsed<u32>,
    pub nickname: Parsed<heapless::String<MAX_NICKNAME_LEN>>,
    pub device: Parsed<DeviceKdl>,
    pub color: Parsed<u32>,
    pub profiles: Parsed<heapless::Vec<Parsed<ProfileKdl>, MAX_PROFILES>>,
    // TODO: add validation: no duplicate pins (including default values), valid pin range
    pub pin_remappings: Parsed<heapless::Vec<Parsed<PinRemappingKdl>, MAX_PIN_REMAPPINGS>>,
}

impl KdlConfigFinalize for ConfigKdl {
    type FinalizeType = Config;

    fn finalize(&self) -> Self::FinalizeType {
        Config {
            meta: Meta {
                version: self.version.value.finalize(),
                nickname: self.nickname.value.finalize(),
                device: self.device.value.finalize(),
                color: self.color.value.finalize(),
                pin_remappings: self.pin_remappings.value.finalize(),
            },
            profiles: self.profiles.value.finalize(),
        }
    }
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
    pub mappings: Parsed<heapless::Vec<Parsed<MappingKdl>, MAX_MAPPINGS>>,
}

#[derive(Default, Debug)]
pub struct MappingKdl {
    pub input_set: heapless::Vec<dpedal_config::DpedalInput, MAX_DPEDAL_INPUTS>,
    pub mode: MappingMode,
    pub output_sequence: heapless::Vec<dpedal_config::ComputerInput, MAX_COMPUTER_INPUTS>,
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
            diagnostics.push(
                ParseDiagnostic::new(source.clone(), node.span()).message("Unexpected format"),
            );
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
                    diagnostics.push(ParseDiagnostic::new(source.clone(), node.span()).message(
                        "Mapping needs to follow format `input -> output` but `->` was not present",
                    ));
                    return Parsed {
                        value: Default::default(),
                        full_span: node.span(),
                        name_span: node.span(),
                        valid: false,
                    };
                };
                let output = output.trim();

                let Some(input) = DpedalInput::from_string_kebab(input) else {
                    diagnostics.push(
                        ParseDiagnostic::new(source.clone(), node.span())
                            .message(format!("Unknown input {input:?}")),
                    );
                    return Parsed {
                        value: Default::default(),
                        full_span: node.span(),
                        name_span: node.span(),
                        valid: false,
                    };
                };
                let input = heapless::Vec::from_slice(&[input]).unwrap();

                let Some((ty, sub_ty)) = output.split_once("-") else {
                    diagnostics.push(
                        ParseDiagnostic::new(source.clone(), node.span())
                            .message(format!("Unknown output {output:?}")),
                    );
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
                            diagnostics.push(
                                ParseDiagnostic::new(source.clone(), node.span())
                                    .message(format!("Unknown output {output:?}")),
                            );
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
                            diagnostics.push(
                                ParseDiagnostic::new(source.clone(), node.span())
                                    .message(format!("Unknown output {output:?}")),
                            );
                            return Parsed {
                                value: Default::default(),
                                full_span: node.span(),
                                name_span: node.span(),
                                valid: false,
                            };
                        }
                    },
                    _ => {
                        diagnostics.push(
                            ParseDiagnostic::new(source.clone(), node.span())
                                .message(format!("Unknown output {output:?}")),
                        );
                        return Parsed {
                            value: Default::default(),
                            full_span: node.span(),
                            name_span: node.span(),
                            valid: false,
                        };
                    }
                };
                let output_sequence = heapless::Vec::from_slice(&[output]).unwrap();
                Parsed {
                    value: MappingKdl {
                        input_set: input,
                        mode: MappingMode::OnPress,
                        output_sequence,
                    },
                    full_span: node.span(),
                    name_span: node.span(),
                    valid: true,
                }
            }
            value => {
                diagnostics.push(ParseDiagnostic::new(source.clone(), node.span()).message(
                    format!("Node contains {value:?} but expected it to contain a string"),
                ));
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

#[cfg(test)]
mod test {
    use std::path::PathBuf;

    use dpedal_config::{
        ComputerInput, Config, DpedalInput, KeyboardInput, MappingMode, Meta, MouseInput,
    };
    use dpedal_config::{Device, Mapping, PinRemapping, Profile};
    use miette::{GraphicalReportHandler, GraphicalTheme};

    use crate::config::load;

    #[test]
    fn test_example_config_loads() {
        load(None).unwrap();
    }

    fn fmt_report(diag: miette::Error) -> String {
        let mut out = String::new();
        GraphicalReportHandler::new_themed(GraphicalTheme::unicode_nocolor())
            .without_syntax_highlighting()
            .with_width(80)
            .render_report(&mut out, diag.as_ref())
            .unwrap();
        out
    }

    #[test]
    fn test_parse_config_bad_nickname() {
        let err = load(Some(PathBuf::from("src/test-configs/bad-nickname.kdl"))).unwrap_err();
        let expected = r#"
  × Failed to parse configuration

Error: 
  × Expected type String but was Integer
   ╭─[bad-nickname.kdl:3:1]
 2 │ device dpedal-v3
 3 │ nickname 5
   · ─────┬────
   ·      ╰── here
 4 │ color 0xFF0000
   ╰────
"#;
        pretty_assertions::assert_eq!(fmt_report(err).trim(), expected.trim());
    }

    #[test]
    fn test_parse_config_success() {
        let config = load(Some(PathBuf::from("src/test-configs/config.kdl"))).unwrap();
        assert_eq!(
            config,
            Config {
                meta: Meta {
                    version: 0,
                    nickname: heapless::String::try_from("My DPedal").unwrap(),
                    device: Device::DpedalV3,
                    color: 0xFF0000,
                    pin_remappings: heapless::Vec::from_iter([
                        PinRemapping {
                            input: DpedalInput::ButtonLeft,
                            pin: 3
                        },
                        PinRemapping {
                            input: DpedalInput::ButtonRight,
                            pin: 20
                        }
                    ])
                },
                profiles: heapless::Vec::from_iter([Profile {
                    mappings: heapless::Vec::from_iter([
                        Mapping {
                            input_set: heapless::Vec::from_iter([DpedalInput::DpadUp]),
                            mode: MappingMode::OnPress,
                            output_sequence: heapless::Vec::from_iter([ComputerInput::Mouse(
                                MouseInput::ScrollUp(20),
                            )]),
                        },
                        Mapping {
                            input_set: heapless::Vec::from_iter([DpedalInput::DpadDown]),
                            mode: MappingMode::OnPress,
                            output_sequence: heapless::Vec::from_iter([ComputerInput::Mouse(
                                MouseInput::ScrollDown(20),
                            )]),
                        },
                        Mapping {
                            input_set: heapless::Vec::from_iter([DpedalInput::DpadLeft]),
                            mode: MappingMode::OnPress,
                            output_sequence: heapless::Vec::from_iter([ComputerInput::Mouse(
                                MouseInput::ScrollLeft(20),
                            )]),
                        },
                        Mapping {
                            input_set: heapless::Vec::from_iter([DpedalInput::DpadRight]),
                            mode: MappingMode::OnPress,
                            output_sequence: heapless::Vec::from_iter([ComputerInput::Mouse(
                                MouseInput::ScrollRight(20),
                            )]),
                        },
                        Mapping {
                            input_set: heapless::Vec::from_iter([DpedalInput::ButtonLeft]),
                            mode: MappingMode::OnPress,
                            output_sequence: heapless::Vec::from_iter([ComputerInput::Keyboard(
                                KeyboardInput::PageUp,
                            )]),
                        },
                        Mapping {
                            input_set: heapless::Vec::from_iter([DpedalInput::ButtonRight]),
                            mode: MappingMode::OnPress,
                            output_sequence: heapless::Vec::from_iter([ComputerInput::Keyboard(
                                KeyboardInput::PageDown,
                            )]),
                        },
                    ]),
                }]),
            }
        );
    }
}
