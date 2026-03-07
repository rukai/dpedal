use web_sys::Element;
use web_sys::HtmlElement;

use crate::document::Document;
use crate::dom_helpers::set_onclick;

#[derive(Clone)]
pub(crate) struct AddRemoveButtons {
    pub add: HtmlElement,
    pub remove: HtmlElement,
}

impl AddRemoveButtons {
    pub fn new(document: &Document) -> Self {
        let add: HtmlElement = document.create_element("button");
        add.set_inner_text("✚");
        add.style().set_css_text("color:green;font-size:2em;");

        let remove: HtmlElement = document.create_element("button");
        remove.set_inner_text("✖");
        remove.style().set_css_text("color:red;font-size:2em;");

        AddRemoveButtons { add, remove }
    }

    pub fn update(&self, container: &Element, max: usize) {
        let count = container.children().length();
        if count >= max as u32 {
            self.add.set_attribute("disabled", "").unwrap();
        } else {
            self.add.remove_attribute("disabled").unwrap();
        }
        if count == 0 {
            self.remove.set_attribute("disabled", "").unwrap();
        } else {
            self.remove.remove_attribute("disabled").unwrap();
        }
    }

    pub fn setup_onclick(
        &self,
        container: Element,
        max: usize,
        create_child: impl Fn() -> Element + 'static,
    ) {
        {
            let container = container.clone();
            let buttons = self.clone();
            set_onclick(
                &self.add,
                Box::new(move || {
                    let child = create_child();
                    container.append_child(&child).unwrap();
                    buttons.update(&container, max);
                }) as Box<dyn FnMut()>,
            );
        }

        {
            let buttons = self.clone();
            set_onclick(
                &self.remove,
                Box::new(move || {
                    if let Some(last) = container.last_element_child() {
                        last.remove();
                    }
                    buttons.update(&container, max);
                }) as Box<dyn FnMut()>,
            );
        }
    }
}
