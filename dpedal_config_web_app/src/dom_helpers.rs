use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use web_sys::Element;
use web_sys::HtmlElement;

use crate::document::Document;

pub(crate) fn clear_children(el: &Element) {
    el.set_inner_html("");
}

pub(crate) fn set_button_on_click(document: &Document, id: &str, closure: Box<dyn FnMut()>) {
    let button: HtmlElement = document.get_element(id);
    set_onclick(&button, closure);
}

pub(crate) fn set_onclick(element: &HtmlElement, closure: Box<dyn FnMut()>) {
    let closure = Closure::wrap(closure);
    element.set_onclick(Some(closure.as_ref().unchecked_ref()));

    // Need to forget closure otherwise the destructor destroys it ;-;
    closure.forget();
}

pub(crate) fn set_onchange(select: &HtmlElement, closure: Box<dyn FnMut()>) {
    let closure = Closure::wrap(closure);
    select.set_onchange(Some(closure.as_ref().unchecked_ref()));

    // Need to forget closure otherwise the destructor destroys it ;-;
    closure.forget();
}
