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
use dpedal_config::MAX_NICKNAME_LEN;
use dpedal_config::MAX_PIN_REMAPPINGS;
use dpedal_config::MAX_PROFILES;
use dpedal_config::Mapping;
use dpedal_config::MouseInput;
use dpedal_config::PinRemapping;
use dpedal_config::Profile;
use dpedal_config::web_config_protocol::Request;
use dpedal_config::web_config_protocol::Response;
use element_iterator::ElementChildIter;
use log::Level;
use rkyv::rancor::Error;
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

use crate::device::Device;
use crate::document::Document;

mod device;
mod document;
mod element_iterator;

#[wasm_bindgen]
pub fn run() {
    std::panic::set_hook(Box::new(console_error_panic_hook::hook));
    console_log::init_with_level(Level::Info).expect("could not initialize logger");

    let document = Document::get();
    set_button_on_click(
        &document,
        "open-device",
        Box::new(move || {
            wasm_bindgen_futures::spawn_local(open_device());
        }) as Box<dyn FnMut()>,
    );
}

/// Opens a webusb device, creating (or replacing the existing) #mapping-section with UI for configuring the device
async fn open_device() {
    let document = Document::get();
    set_error(&document, "");

    let device = match Device::new().await {
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

    let config_div = document
        .0
        .get_element_by_id("mapping-section")
        .unwrap_or_else(|| {
            let config_div: Element = document.create_element("div");
            config_div.set_id("mapping-section");
            config_div
        });
    config_div.set_inner_html(
        r#"
            <label>Nickname: </label>
            <input type="text" id="device_name" style="font-size:2em;">
            <input type="color" id="device_color">

            <div id="profiles-container"></div>
            <button id="add-profile" style="margin-top:2em;">Add Profile</button>
            <button id="save">Save</button>
            <span id="save-result" style="font-size:1.5em;"></span>
            "#,
    );

    let config_div: HtmlElement = config_div.dyn_into().unwrap();

    let app_div: Element = document.get_element("config-app");
    app_div.append_child(&config_div).unwrap();

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

    let device = Rc::new(device);
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

            let input_cell = cells.next().unwrap();
            let selects_container = ElementChildIter::new(&input_cell).nth(2).unwrap();
            let input = ArrayVec::from_iter(
                ElementChildIter::<HtmlSelectElement>::new(&selects_container)
                    .map(|s| DpedalInput::from_string(&s.value()).unwrap()),
            );

            let output = parse_output_cell(&cells.next().unwrap());
            mappings.push(Mapping { input, output });
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

fn parse_output_cell(output_cell: &Element) -> ArrayVec<ComputerInput, MAX_COMPUTER_INPUTS> {
    ElementChildIter::new(output_cell)
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
    profile_section
        .style()
        .set_css_text("margin-top:3em;margin-bottom:0.5em");

    let header_row: HtmlElement = document.create_element("div");
    header_row
        .style()
        .set_css_text("display:flex; align-items:center; gap:1em;margin-bottom:0.5em");

    let heading: HtmlElement = document.create_element("h2");
    heading.style().set_css_text("margin:0;");
    heading.set_inner_text(&format!("Profile {index}"));
    header_row.append_child(&heading).unwrap();

    let remove_button: HtmlElement = document.create_element("button");
    remove_button.set_inner_text("Remove");
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

    profile_section.append_child(&header_row).unwrap();

    let table: HtmlElement = document.create_element("table");
    table.set_inner_html("<tr><th>Input</th><th>Output</th></tr>");
    table.style().set_css_text("margin:0;");
    for mapping in &profile.mappings {
        let row = create_row(document, mapping);
        table.append_child(&row).unwrap();
    }
    profile_section.append_child(&table).unwrap();

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

pub fn set_error(document: &Document, error_message: &str) {
    let error: HtmlElement = document.get_element("error");
    error.set_inner_text(error_message);
}

fn create_row(document: &Document, mapping: &Mapping) -> Element {
    let tr: Element = document.create_element("tr");

    tr.append_child(&create_row_input(document, &mapping.input))
        .unwrap();
    tr.append_child(&create_row_output(document, &mapping.output))
        .unwrap();

    tr
}

fn create_dpedal_input_select(document: &Document, value: &DpedalInput) -> HtmlSelectElement {
    let select: HtmlSelectElement = document.create_element("select");
    let mut options = String::new();
    for variant in DpedalInput::iter() {
        let name: &str = (&variant).into();
        options.push_str(&format!("<option value=\"{name}\">{name}</option>"));
    }
    select.set_inner_html(&options);
    select.style().set_css_text("font-size:2em;");
    select.set_value(<&str>::from(value));
    select
}

fn update_input_buttons(
    container: &Element,
    add_button: &HtmlElement,
    remove_button: &HtmlElement,
) {
    let count = container.children().length();
    if count >= MAX_DPEDAL_INPUTS as u32 {
        add_button.set_attribute("disabled", "").unwrap();
    } else {
        add_button.remove_attribute("disabled").unwrap();
    }
    if count == 0 {
        remove_button.set_attribute("disabled", "").unwrap();
    } else {
        remove_button.remove_attribute("disabled").unwrap();
    }
}

fn create_row_input(
    document: &Document,
    inputs: &ArrayVec<DpedalInput, MAX_DPEDAL_INPUTS>,
) -> Element {
    let td: HtmlElement = document.create_element("td");
    td.style()
        .set_css_text("display:flex; align-items:flex-start;");

    let add_button: HtmlElement = document.create_element("button");
    add_button.set_inner_text("✚");
    add_button
        .style()
        .set_css_text("color:green;font-size:2em;");
    td.append_child(&add_button).unwrap();

    let remove_button: HtmlElement = document.create_element("button");
    remove_button.set_inner_text("✖");
    remove_button
        .style()
        .set_css_text("color:red;font-size:2em;");
    td.append_child(&remove_button).unwrap();

    let container: Element = document.create_element("div");
    for input in inputs {
        let select = create_dpedal_input_select(document, input);
        container.append_child(&select).unwrap();
    }
    td.append_child(&container).unwrap();

    update_input_buttons(&container, &add_button, &remove_button);

    {
        let container = container.clone();
        let add_button_clone = add_button.clone();
        let remove_button_clone = remove_button.clone();
        set_onclick(
            &add_button,
            Box::new(move || {
                let document = Document::get();
                let select = create_dpedal_input_select(&document, &DpedalInput::default());
                container.append_child(&select).unwrap();
                update_input_buttons(&container, &add_button_clone, &remove_button_clone);
            }) as Box<dyn FnMut()>,
        );
    }

    {
        let container = container.clone();
        let add_button_clone = add_button.clone();
        let remove_button_clone = remove_button.clone();
        set_onclick(
            &remove_button,
            Box::new(move || {
                if let Some(last) = container.last_element_child() {
                    last.remove();
                }
                update_input_buttons(&container, &add_button_clone, &remove_button_clone);
            }) as Box<dyn FnMut()>,
        );
    }

    td.into()
}

fn create_row_output<const CAP: usize>(
    document: &Document,
    outputs: &ArrayVec<ComputerInput, CAP>,
) -> Element {
    let td: Element = document.create_element("td");
    let span: Element = document.create_element("span");
    td.append_child(&span).unwrap();

    for output in outputs {
        setup_single_output_span(&span, output);
    }

    td
}

/// Create or recreate a single output span.
/// The output cell of the mapping table can contain many of these spans, each corresponding to a distinct output or step in a dpedal macro.
fn setup_single_output_span(span: &Element, output: &ComputerInput) {
    let document = Document::get();

    clear_children(span);

    // Add new children
    let select_type: HtmlSelectElement = document.create_element("select");
    select_type.set_inner_html(
        "
<option value=\"keyboard\">⌨️</option>
<option value=\"mouse\">🖱️</option>
<option value=\"control\">⚙️</option>
",
    );
    select_type.style().set_css_text("font-size:2em;");
    select_type.set_value(match output {
        ComputerInput::None => {
            return;
        }
        ComputerInput::Mouse(_) => "mouse",
        ComputerInput::Keyboard(_) => "keyboard",
        ComputerInput::Control(_) => "control",
    });
    span.append_child(&select_type).unwrap();

    let select_subtype: HtmlSelectElement = document.create_element("select");
    select_subtype.style().set_css_text("font-size:2em;");
    span.append_child(&select_subtype).unwrap();

    let subtype_fields_span: Element = document.create_element("span");

    match output {
        ComputerInput::None => {}
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
                        MouseInput::from_string(&select_subtype_clone.value(), "10").unwrap();
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
                "control" => ComputerInput::Control(Default::default()),
                _ => ComputerInput::None,
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
        DPedalControl::SetProfile(x) => Some(*x as i32),
        DPedalControl::DoNothing => None,
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
        input_field.style().set_css_text("font-size:2em;");
        input_field.set_min("0");
        input_field.set_max("1000");
        input_field.set_required(true);
        span.append_child(&input_field).unwrap();
    }
}

fn clear_children(el: &Element) {
    while let Some(child) = el.first_element_child() {
        child.remove();
    }
}

fn set_button_on_click(document: &Document, id: &str, closure: Box<dyn FnMut()>) {
    let button: HtmlElement = document.get_element(id);
    set_onclick(&button, closure);
}

fn set_onclick(element: &HtmlElement, closure: Box<dyn FnMut()>) {
    let closure = Closure::wrap(closure);
    element.set_onclick(Some(closure.as_ref().unchecked_ref()));

    // Need to forget closure otherwise the destructor destroys it ;-;
    closure.forget();
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

fn set_onchange(select: &HtmlElement, closure: Box<dyn FnMut()>) {
    let closure = Closure::wrap(closure);
    select.set_onchange(Some(closure.as_ref().unchecked_ref()));

    // Need to forget closure otherwise the destructor destroys it ;-;
    closure.forget();
}

/// The fields from the device's config that are not editable via the UI and must be
/// round-tripped back when saving, so they aren't silently discarded.
#[derive(Clone)]
struct PreservedConfig {
    version: u32,
    device: DPedalDevice,
    pin_remappings: ArrayVec<PinRemapping, MAX_PIN_REMAPPINGS>,
}
