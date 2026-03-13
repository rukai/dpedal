use arrayvec::ArrayString;
use arrayvec::ArrayVec;
use dpedal_config::ComputerInput;
use dpedal_config::Config;
use dpedal_config::DPedalControl;
use dpedal_config::Device as DPedalDevice;
use dpedal_config::DpedalInput;
use dpedal_config::KeyboardInput;
use dpedal_config::MAX_COMPUTER_INPUTS;
use dpedal_config::MAX_DPEDAL_INPUTS;
use dpedal_config::MAX_MAPPINGS;
use dpedal_config::MAX_NICKNAME_LEN;
use dpedal_config::MAX_PIN_REMAPPINGS;
use dpedal_config::MAX_PROFILES;
use dpedal_config::Mapping;
use dpedal_config::MappingMode;
use dpedal_config::MouseInput;
use dpedal_config::PinRemapping;
use dpedal_config::Profile;
use dpedal_config::web_config_protocol::Request;
use dpedal_config::web_config_protocol::Response;
use element_iterator::ElementChildIter;
use log::Level;
use rkyv::rancor::Error;
use std::cell::RefCell;
use std::rc::Rc;
use std::str::FromStr;
use strum::IntoEnumIterator;
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::Element;
use web_sys::HtmlElement;
use web_sys::HtmlInputElement;
use web_sys::HtmlSelectElement;

use crate::add_remove_buttons::AddRemoveButtons;
use crate::device::{Device, DeviceList};
use crate::document::Document;
use crate::dom_helpers::{clear_children, set_button_on_click, set_onchange, set_onclick};

mod add_remove_buttons;
mod device;
mod document;
mod dom_helpers;
mod element_iterator;

#[wasm_bindgen]
pub fn run() {
    std::panic::set_hook(Box::new(console_error_panic_hook::hook));
    console_log::init_with_level(Level::Info).expect("could not initialize logger");

    let document = Document::get();
    inject_css(&document);
    let devices: DeviceList = Rc::new(RefCell::new(Vec::new()));
    set_button_on_click(
        &document,
        "open-device",
        Box::new(move || {
            let devices = devices.clone();
            wasm_bindgen_futures::spawn_local(open_device(devices));
        }) as Box<dyn FnMut()>,
    );
}

/// Opens a webusb device and setup + unhide the configuration UI sections
async fn open_device(devices: DeviceList) {
    let document = Document::get();
    set_error(&document, "");

    let device = match Device::new(&devices).await {
        Ok(device) => device,
        Err(e) => {
            set_error(&document, &e);
            return;
        }
    };

    let config = match request_get_config(&device).await {
        Ok(x) => x,
        Err(err) => {
            log::error!("Failed to request config from device {err}");
            web_sys::window()
                .unwrap()
                .alert_with_message("Config on the device is not present, outdated, or corrupt. Default config will be restored.")
                .unwrap();
            Default::default()
        }
    };

    // Unhide sections (hidden by default in HTML until device is opened)
    for id in ["meta-section", "mapping-content", "save-section"] {
        let section: Element = document.get_element(id);
        section.remove_attribute("hidden").unwrap();
    }

    // Clear previous state for re-open case
    let profiles_container: Element = document.get_element("profiles-container");
    clear_children(&profiles_container);
    let save_result: Element = document.get_element("save-result");
    save_result.set_text_content(None);

    let device_type: HtmlElement = document.get_element("device_type");
    device_type.set_inner_text(<&str>::from(&config.device));

    let name: HtmlInputElement = document.get_element("device_name");
    name.set_value(config.nickname.as_ref());

    let color: HtmlInputElement = document.get_element("device_color");
    color.set_value(&format!("#{:0>6x}", config.color));

    for (i, profile) in config.profiles.iter().enumerate() {
        gen_for_profile(&document, profile, i);
    }
    update_add_profile_button(&document);
    log::info!("device config {:#?}", config);

    set_button_on_click(
        &document,
        "add-profile",
        Box::new(move || {
            let document = Document::get();
            let container: Element = document.get_element("profiles-container");
            let count = container.children().length() as usize;
            if count < MAX_PROFILES {
                gen_for_profile(&document, &Profile::default(), count);
                update_add_profile_button(&document);
            }
        }) as Box<dyn FnMut()>,
    );

    let preserved = PreservedConfig {
        version: config.version,
        device: config.device,
        pin_remappings: config.pin_remappings,
    };

    set_button_on_click(
        &document,
        "save",
        Box::new(move || {
            let device = device.clone();
            let preserved = preserved.clone();
            wasm_bindgen_futures::spawn_local(write_config_task(device, preserved));
        }) as Box<dyn FnMut()>,
    );

    log::info!("Setup complete");
}

async fn sleep(millis: i32) {
    let promise = js_sys::Promise::new(&mut |resolve, _| {
        web_sys::window()
            .unwrap()
            .set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, millis)
            .unwrap();
    });

    JsFuture::from(promise).await.unwrap();
}

async fn write_config_task(device: Rc<Device>, preserved: PreservedConfig) {
    let document = Document::get();
    let save_result: HtmlElement = document.get_element("save-result");
    save_result.set_inner_html("🌀");

    if let Err(err) = write_config(&document, device, preserved).await {
        save_result.set_inner_html("❌");
        set_error(&document, &err);
        return;
    }

    save_result.set_inner_html("✅");
    sleep(500).await;
    for i in (0..100).rev() {
        save_result
            .style()
            .set_property("opacity", &format!("{i}%"))
            .unwrap();
        sleep(10).await;
    }
    save_result.set_inner_html("");
    save_result.style().set_property("opacity", "100%").unwrap();
}

async fn write_config(
    document: &Document,
    device: Rc<Device>,
    preserved: PreservedConfig,
) -> Result<(), String> {
    let profiles_container: Element = document.get_element("profiles-container");
    let mut profiles = ArrayVec::new();

    for profile_section in ElementChildIter::new(&profiles_container) {
        let mut section_children = ElementChildIter::new(&profile_section);
        section_children.next(); // skip header_row
        let table = section_children.next().unwrap();

        let mut mappings = ArrayVec::new();

        // Iterate over rows, skipping the header
        for row in ElementChildIter::new(&table).skip(1) {
            let mut cells = ElementChildIter::new(&row);

            let mode = parse_mode_cell(&cells.next().unwrap());

            let input_cell: Element = cells.next().unwrap();
            let wrapper = input_cell.children().item(0).unwrap();
            let selects_container = wrapper.children().item(2).unwrap();
            let input = ArrayVec::from_iter(ElementChildIter::new(&selects_container).map(
                |wrapper: Element| {
                    let s: HtmlSelectElement =
                        wrapper.first_element_child().unwrap().dyn_into().unwrap();
                    DpedalInput::from_string(&s.value()).unwrap()
                },
            ));
            let output_sequence = parse_output_cell(&cells.next().unwrap());
            mappings.push(Mapping {
                input_set: input,
                mode,
                output_sequence,
            });
        }

        profiles.push(Profile { mappings });
    }

    let name: HtmlInputElement = document.get_element("device_name");
    let nickname = ArrayString::from(&name.value()).map_err(|_| {
        format!(
            "nickname must be <= {MAX_NICKNAME_LEN} characters long, but was {} characters long",
            name.value().len()
        )
    })?;

    let color_element: HtmlInputElement = document.get_element("device_color");
    let color = u32::from_str_radix(color_element.value().strip_prefix("#").unwrap(), 16).unwrap();

    let config = Config {
        version: preserved.version,
        nickname,
        device: preserved.device,
        color,
        profiles,
        pin_remappings: preserved.pin_remappings,
    };

    let config_bytes =
        ArrayVec::from_iter(rkyv::to_bytes::<Error>(&config).unwrap().iter().cloned());
    device
        .send_request(&Request::SetConfig(config_bytes))
        .await?;
    log::info!("config written {:#?}", config);

    Ok(())
}

fn parse_mode_cell(mode_cell: &Element) -> MappingMode {
    let children = mode_cell.children();
    let variant_name = children
        .item(0)
        .unwrap()
        .dyn_into::<HtmlSelectElement>()
        .unwrap()
        .value();
    let fields_span = children.item(1).unwrap();
    let ms_value = ElementChildIter::<HtmlInputElement>::new(&fields_span)
        .next()
        .map(|x| x.value())
        .unwrap_or_default();
    MappingMode::from_string(&variant_name, &ms_value).unwrap_or_default()
}

fn parse_output_cell(output_cell: &Element) -> ArrayVec<ComputerInput, MAX_COMPUTER_INPUTS> {
    let wrapper = output_cell.children().item(0).unwrap();
    let container = wrapper.children().item(2).unwrap();
    ElementChildIter::new(&container)
        .flat_map(|span| parse_output_span(&span))
        .collect()
}

fn parse_output_span(output_span: &Element) -> Option<ComputerInput> {
    let children = output_span.children();
    let ty_value = children
        .item(0)?
        .dyn_into::<HtmlSelectElement>()
        .unwrap()
        .value();
    let sub_ty_value = children
        .item(1)?
        .dyn_into::<HtmlSelectElement>()
        .unwrap()
        .value();
    let sub_ty_fields_span = children.item(2)?;

    match ty_value.as_str() {
        "mouse" => {
            let field = ElementChildIter::<HtmlInputElement>::new(&sub_ty_fields_span)
                .next()
                .map(|x| x.value())
                .unwrap_or("".into());
            MouseInput::from_string(&sub_ty_value, &field).map(ComputerInput::Mouse)
        }
        "keyboard" => KeyboardInput::from_str(&sub_ty_value)
            .ok()
            .map(ComputerInput::Keyboard),
        "control" => {
            let field = ElementChildIter::<HtmlInputElement>::new(&sub_ty_fields_span)
                .next()
                .map(|x| x.value())
                .unwrap_or("".into());
            DPedalControl::from_string(&sub_ty_value, &field).map(ComputerInput::Control)
        }
        _ => None,
    }
}

async fn request_get_config(device: &Device) -> Result<Config, String> {
    let response = device.send_request(&Request::GetConfig).await?;
    match response {
        Response::GetConfig(config_bytes) => {
            // TODO: Is Align required in some circumstances?
            rkyv::from_bytes::<Config, rkyv::rancor::Error>(
                &config_bytes.map_err(|e| format!("{e:?}"))?,
            )
            .map_err(|e| format!("{e:?}"))
        }
        Response::SetConfig => panic!("Unexpected dpedal response"),
        Response::ProtocolError => panic!("dpedal protocol error"),
    }
}

fn gen_for_profile(document: &Document, profile: &Profile, index: usize) {
    let profiles_container: Element = document.get_element("profiles-container");

    let profile_section: HtmlElement = document.create_element("div");
    profile_section
        .set_attribute("class", "profile-section")
        .unwrap();
    let header_row: HtmlElement = document.create_element("div");
    header_row.style().set_css_text(
        "display:flex; align-items:center; gap:1em;margin-bottom:0.5em;padding-left:1em",
    );

    let heading: HtmlElement = document.create_element("h2");
    heading.style().set_css_text("margin:0;");
    heading.set_inner_text(&format!("Profile {index}"));
    header_row.append_child(&heading).unwrap();

    let remove_button: HtmlElement = document.create_element("button");
    remove_button.set_inner_text("Remove");
    remove_button.set_class_name("red-button");
    let profile_section_clone = profile_section.clone();
    set_onclick(
        &remove_button,
        Box::new(move || {
            profile_section_clone.remove();
            let document = Document::get();
            renumber_profiles(&document);
            update_add_profile_button(&document);
        }) as Box<dyn FnMut()>,
    );
    header_row.append_child(&remove_button).unwrap();

    let table: HtmlElement = document.create_element("table");
    table.set_class_name("mapping-table");
    table.set_inner_html(r#"<tr>
        <th style='width:15em'><abbr title='Controls when the output sequence is triggered.'>Mode</abbr></th>
        <th style='width:22%'><abbr title='Each cell contains a series of chorded pedal inputs that must be all held to activate the output cell.'>Input</abbr></th>
        <th><abbr title='Each cell contains a series of PC inputs forming a macro.'>Output</abbr></th>
        <th style='width:4.3em'>Remove</th>
    </tr>"#);

    let add_row_button: HtmlElement = document.create_element("button");
    add_row_button.set_inner_text("Add row");
    add_row_button
        .set_attribute("class", "add-row-button green-button")
        .unwrap();
    add_row_button.style().set_css_text("margin-top:1em");
    let table_clone = table.clone();
    let add_row_button_clone = add_row_button.clone();
    set_onclick(
        &add_row_button,
        Box::new(move || {
            let document = Document::get();
            let row = create_row(&document, &Mapping::default());
            table_clone.append_child(&row).unwrap();
            update_add_row_button(&add_row_button_clone, &table_clone);
        }) as Box<dyn FnMut()>,
    );
    profile_section.append_child(&header_row).unwrap();

    for mapping in &profile.mappings {
        let row = create_row(document, mapping);
        table.append_child(&row).unwrap();
    }
    profile_section.append_child(&table).unwrap();
    profile_section.append_child(&add_row_button).unwrap();

    update_add_row_button(&add_row_button, &table);

    profiles_container.append_child(&profile_section).unwrap();
}

fn update_add_profile_button(document: &Document) {
    let container: Element = document.get_element("profiles-container");
    let count = container.children().length() as usize;
    let add_button: HtmlElement = document.get_element("add-profile");
    if count < MAX_PROFILES {
        add_button.remove_attribute("disabled").unwrap();
    } else {
        add_button.set_attribute("disabled", "").unwrap();
    }
}

fn update_add_row_button(button: &HtmlElement, table: &HtmlElement) {
    let row_count = table.children().length() as usize - 1; // subtract header
    if row_count < MAX_MAPPINGS {
        button.remove_attribute("disabled").unwrap();
    } else {
        button.set_attribute("disabled", "").unwrap();
    }
}

pub fn set_error(document: &Document, error_message: &str) {
    let error: HtmlElement = document.get_element("error");
    error.set_inner_text(error_message);
}

fn create_row(document: &Document, mapping: &Mapping) -> Element {
    let tr: Element = document.create_element("tr");

    tr.append_child(&create_row_mode(document, &mapping.mode))
        .unwrap();
    tr.append_child(&create_row_input(document, &mapping.input_set))
        .unwrap();
    tr.append_child(&create_row_output(document, &mapping.output_sequence))
        .unwrap();

    let remove_td: HtmlElement = document.create_element("td");
    remove_td.style().set_css_text("text-align:center;");
    let remove_btn: HtmlElement = document.create_element("button");
    remove_btn.set_inner_text("✖");
    remove_btn.set_title("Remove Row");
    remove_btn.set_class_name("red-button mapping-table-element");
    let tr_clone = tr.clone();
    set_onclick(
        &remove_btn,
        Box::new(move || {
            let table = tr_clone.parent_element().unwrap();
            let profile_section = table.parent_element().unwrap();
            tr_clone.remove();
            let table: HtmlElement = table.dyn_into().unwrap();
            let add_row_btn = profile_section
                .query_selector(".add-row-button")
                .unwrap()
                .unwrap()
                .dyn_into::<HtmlElement>()
                .unwrap();
            update_add_row_button(&add_row_btn, &table);
        }) as Box<dyn FnMut()>,
    );
    remove_td.append_child(&remove_btn).unwrap();
    tr.append_child(&remove_td).unwrap();

    tr
}

fn create_row_mode(document: &Document, mode: &MappingMode) -> Element {
    let td: Element = document.create_element("td");

    let select: HtmlSelectElement = document.create_element("select");
    let mut options = String::new();
    for variant in MappingMode::iter() {
        let name: &'static str = variant.into();
        options.push_str(&format!("<option value=\"{name}\">{name}</option>"));
    }
    select.set_inner_html(&options);
    select.set_class_name("mapping-table-element");
    let mode_name: &'static str = mode.into();
    select.set_value(mode_name);

    let fields_span: Element = document.create_element("span");
    setup_mode_fields(&fields_span, mode);

    let select_clone = select.clone();
    let fields_span_clone = fields_span.clone();
    set_onchange(
        &select,
        Box::new(move || {
            let dummy = MappingMode::from_string(&select_clone.value(), "500").unwrap_or_default();
            setup_mode_fields(&fields_span_clone, &dummy);
        }) as Box<dyn FnMut()>,
    );

    td.append_child(&select).unwrap();
    td.append_child(&fields_span).unwrap();
    td
}

fn setup_mode_fields(span: &Element, mode: &MappingMode) {
    let value = match mode {
        MappingMode::OnHoldMillisUntilRelease(x)
        | MappingMode::MacroOnTapMillis(x)
        | MappingMode::MacroOnDoubleTapMillis(x)
        | MappingMode::MacroOnHoldMillis(x) => Some(*x as i32),
        _ => None,
    };
    clear_children(span);
    if let Some(x) = value {
        let document = Document::get();
        // unlike other fields that appear to the right of the dropdown,
        // the mode fields are intentionally placed on a newline due to the short fixed width of the mode column
        let input_field: HtmlInputElement = document.create_element("input");
        input_field.style().set_css_text("margin-top:4px;");
        input_field.set_type("number");
        input_field.set_value(&x.to_string());
        input_field.set_class_name("mapping-table-element");
        input_field.set_min("0");
        input_field.set_max("65535");
        input_field.set_required(true);
        span.append_child(&input_field).unwrap();
    }
}

fn create_dpedal_input_select(document: &Document, value: &DpedalInput) -> HtmlSelectElement {
    let select: HtmlSelectElement = document.create_element("select");
    let mut options = String::new();
    for variant in DpedalInput::iter() {
        let name: &str = (&variant).into();
        options.push_str(&format!("<option value=\"{name}\">{name}</option>"));
    }
    select.set_inner_html(&options);
    select.set_class_name("mapping-table-element");
    select.set_value(<&str>::from(value));
    select
}

fn create_row_input(
    document: &Document,
    inputs: &ArrayVec<DpedalInput, MAX_DPEDAL_INPUTS>,
) -> Element {
    let td: Element = document.create_element("td");

    let wrapper: HtmlElement = document.create_element("div");
    wrapper
        .style()
        .set_css_text("display:flex; align-items:flex-start;");

    let buttons = AddRemoveButtons::new(document, "Input");
    wrapper.append_child(&buttons.add).unwrap();
    wrapper.append_child(&buttons.remove).unwrap();

    let container: Element = document.create_element("div");
    container.set_class_name("chord-container");
    for input in inputs {
        let select = create_dpedal_input_select(document, input);
        let inner_wrapper: Element = document.create_element("span");
        inner_wrapper.append_child(&select).unwrap();
        container.append_child(&inner_wrapper).unwrap();
    }
    wrapper.append_child(&container).unwrap();
    td.append_child(&wrapper).unwrap();

    buttons.update(&container, MAX_DPEDAL_INPUTS);
    buttons.setup_onclick(container, MAX_DPEDAL_INPUTS, || {
        let document = Document::get();
        let select = create_dpedal_input_select(&document, &DpedalInput::default());
        let wrapper: Element = document.create_element("span");
        wrapper.append_child(&select).unwrap();
        wrapper
    });

    td
}

fn create_row_output(
    document: &Document,
    outputs: &ArrayVec<ComputerInput, MAX_COMPUTER_INPUTS>,
) -> Element {
    let td: Element = document.create_element("td");

    let wrapper: HtmlElement = document.create_element("div");
    wrapper
        .style()
        .set_css_text("display:flex; align-items:flex-start;");

    let buttons = AddRemoveButtons::new(document, "Output");
    wrapper.append_child(&buttons.add).unwrap();
    wrapper.append_child(&buttons.remove).unwrap();

    let container: Element = document.create_element("div");
    container.set_class_name("sequence-container");
    for output in outputs {
        let span: Element = document.create_element("span");
        setup_single_output_span(&span, output);
        container.append_child(&span).unwrap();
    }
    wrapper.append_child(&container).unwrap();
    td.append_child(&wrapper).unwrap();

    buttons.update(&container, MAX_COMPUTER_INPUTS);
    buttons.setup_onclick(container, MAX_COMPUTER_INPUTS, || {
        let document = Document::get();
        let span: Element = document.create_element("span");
        setup_single_output_span(&span, &ComputerInput::Keyboard(Default::default()));
        span
    });

    td
}

/// Create or recreate a single output span.
/// The output cell of the mapping table can contain many of these spans, each corresponding to a distinct output or step in a dpedal macro.
fn setup_single_output_span(span: &Element, output: &ComputerInput) {
    let document = Document::get();

    clear_children(span);
    let span_html: &HtmlElement = span.dyn_ref().unwrap();
    span_html.style().set_css_text("white-space:nowrap;");

    // Add new children
    let select_type: HtmlSelectElement = document.create_element("select");
    select_type.set_inner_html(
        "
<option value=\"keyboard\">⌨️</option>
<option value=\"mouse\">🖱️</option>
<option value=\"control\">⚙️</option>
",
    );
    select_type.set_class_name("mapping-table-element");
    select_type.set_value(match output {
        ComputerInput::Mouse(_) => "mouse",
        ComputerInput::Keyboard(_) => "keyboard",
        ComputerInput::Control(_) => "control",
    });
    span.append_child(&select_type).unwrap();

    let select_subtype: HtmlSelectElement = document.create_element("select");
    select_subtype.set_class_name("mapping-table-element");
    span.append_child(&select_subtype).unwrap();

    let subtype_fields_span: Element = document.create_element("span");

    match output {
        ComputerInput::Mouse(mouse_input) => {
            let mut options = String::new();
            for variant in MouseInput::iter() {
                let name: &str = (&variant).into();
                options.push_str(&format!("<option value=\"{name}\">{name}</option>"));
            }
            select_subtype.set_inner_html(&options);
            select_subtype.set_value(<&str>::from(mouse_input));
            setup_subtype_fields_mouse(&subtype_fields_span, mouse_input);

            let select_subtype_clone = select_subtype.clone();
            let subtype_fields_span = subtype_fields_span.clone();
            set_onchange(
                &select_subtype,
                Box::new(move || {
                    // Create a default for the selected MouseInput
                    let mouse_input =
                        MouseInput::from_string(&select_subtype_clone.value(), "20").unwrap();
                    setup_subtype_fields_mouse(&subtype_fields_span, &mouse_input);
                }) as Box<dyn FnMut()>,
            );
        }
        ComputerInput::Keyboard(keyboard_input) => {
            let mut options = String::new();
            options.push_str("<optgroup label=\"Common Keys\">");
            for variant in KeyboardInput::common_iter() {
                options.push_str(&format!(
                    "<option value=\"{variant:?}\">{variant:?}</option>"
                ));
            }
            options.push_str("</optgroup>");
            options.push_str("<optgroup label=\"Obscure Keys\">");
            for variant in KeyboardInput::obscure_iter() {
                options.push_str(&format!(
                    "<option value=\"{variant:?}\">{variant:?}</option>"
                ));
            }
            options.push_str("</optgroup>");
            select_subtype.set_inner_html(&options);
            select_subtype.set_value(&format!("{keyboard_input:?}"));
        }
        ComputerInput::Control(control) => {
            let mut options = String::new();
            for variant in DPedalControl::iter() {
                let name: &str = (&variant).into();
                options.push_str(&format!("<option value=\"{name}\">{name}</option>"));
            }
            select_subtype.set_inner_html(&options);
            select_subtype.set_value(<&str>::from(control));
            setup_subtype_fields_control(&subtype_fields_span, control);

            let select_subtype_clone = select_subtype.clone();
            let subtype_fields_span = subtype_fields_span.clone();
            set_onchange(
                &select_subtype,
                Box::new(move || {
                    // Create a default for the selected DPedalControl
                    let control =
                        DPedalControl::from_string(&select_subtype_clone.value(), "0").unwrap();
                    setup_subtype_fields_control(&subtype_fields_span, &control);
                }) as Box<dyn FnMut()>,
            );
        }
    }
    span.append_child(&subtype_fields_span).unwrap();

    let span = span.clone();
    set_onchange(
        &select_type,
        Box::new(move || {
            let select_type = ElementChildIter::<HtmlSelectElement>::new(&span)
                .next()
                .unwrap();
            let output = match select_type.value().as_str() {
                "mouse" => ComputerInput::Mouse(Default::default()),
                "keyboard" => ComputerInput::Keyboard(Default::default()),
                _ => ComputerInput::Control(Default::default()),
            };
            setup_single_output_span(&span, &output);
        }) as Box<dyn FnMut()>,
    );
}

fn setup_subtype_fields_mouse(span: &Element, mouse_input: &MouseInput) {
    let value = match mouse_input {
        MouseInput::ScrollUp(x)
        | MouseInput::ScrollDown(x)
        | MouseInput::ScrollRight(x)
        | MouseInput::ScrollLeft(x)
        | MouseInput::MoveUp(x)
        | MouseInput::MoveDown(x)
        | MouseInput::MoveRight(x)
        | MouseInput::MoveLeft(x) => Some(*x as i32),
        MouseInput::ClickLeft | MouseInput::ClickMiddle | MouseInput::ClickRight => None,
    };
    setup_subtype_fields_numeric(span, value);
}

fn setup_subtype_fields_control(span: &Element, control: &DPedalControl) {
    let value = match control {
        DPedalControl::AfterMillisHold(x) | DPedalControl::AfterMillisRelease(x) => Some(*x as i32),
        DPedalControl::SetProfile(x) => Some(*x as i32),
        DPedalControl::Restart => None,
    };
    setup_subtype_fields_numeric(span, value);
}

fn setup_subtype_fields_numeric(span: &Element, value: Option<i32>) {
    clear_children(span);
    if let Some(x) = value {
        let document = Document::get();
        let input_field: HtmlInputElement = document.create_element("input");
        input_field.set_type("number");
        input_field.set_value(&x.to_string());
        input_field.set_class_name("mapping-table-element");
        input_field.set_min("0");
        input_field.set_max("65535");
        input_field.set_required(true);
        span.append_child(&input_field).unwrap();
    }
}

fn renumber_profiles(document: &Document) {
    let container: Element = document.get_element("profiles-container");
    for (i, profile_section) in ElementChildIter::new(&container).enumerate() {
        let header_row = ElementChildIter::new(&profile_section).next().unwrap();
        let heading = ElementChildIter::<HtmlElement>::new(&header_row)
            .next()
            .unwrap();
        heading.set_inner_text(&format!("Profile {i}"));
    }
}

/// The fields from the device's config that are not editable via the UI and must be
/// round-tripped back when saving, so they aren't silently discarded.
#[derive(Clone)]
struct PreservedConfig {
    version: u32,
    device: DPedalDevice,
    pin_remappings: ArrayVec<PinRemapping, MAX_PIN_REMAPPINGS>,
}

fn inject_css(document: &Document) {
    let style: Element = document.create_element("style");
    style.set_text_content(Some(include_str!("style.css")));
    document
        .0
        .query_selector("head")
        .unwrap()
        .unwrap()
        .append_child(&style)
        .unwrap();
}
