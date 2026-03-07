use wasm_bindgen::JsCast;

pub struct Document(pub(crate) web_sys::Document);

impl Document {
    pub fn get() -> Document {
        Document(web_sys::window().unwrap().document().unwrap())
    }

    pub fn create_element<T: JsCast>(&self, tag: &str) -> T {
        self.0.create_element(tag).unwrap().dyn_into().unwrap()
    }

    pub fn get_element<T: JsCast>(&self, id: &str) -> T {
        self.0.get_element_by_id(id).unwrap().dyn_into().unwrap()
    }
}
