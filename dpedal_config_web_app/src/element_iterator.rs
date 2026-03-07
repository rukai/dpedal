use std::marker::PhantomData;
use wasm_bindgen::JsCast;
use web_sys::{Element, HtmlCollection};

pub struct ElementChildIter<T> {
    collection: HtmlCollection,
    index: u32,
    length: u32,
    _phantom: PhantomData<T>,
}

impl<T: JsCast> ElementChildIter<T> {
    /// Create a new iterator over the children of an element
    pub fn new(element: &Element) -> Self {
        let collection = element.children();
        let length = collection.length();

        Self {
            collection,
            index: 0,
            length,
            _phantom: PhantomData,
        }
    }
}

impl<T: JsCast> Iterator for ElementChildIter<T> {
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        if self.index < self.length {
            let element = self.collection.item(self.index);
            self.index += 1;
            element.map(|e| {
                e.dyn_into::<T>().unwrap_or_else(|_| {
                    panic!(
                        "Failed to convert Element into {}, this conversion may be decided by type inference, so consider explicitly using `ElementChildIterator<Element>` if intentionally iterating over children of varying types",
                        std::any::type_name::<T>()
                    )
                })
            })
        } else {
            None
        }
    }
}
