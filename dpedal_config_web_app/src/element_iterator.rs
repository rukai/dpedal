use web_sys::{Element, HtmlCollection};

pub struct ElementChildIter {
    collection: HtmlCollection,
    index: u32,
    length: u32,
}

impl ElementChildIter {
    /// Create a new iterator over the children of an element
    pub fn new(element: &Element) -> Self {
        let collection = element.children();
        let length = collection.length();

        Self {
            collection,
            index: 0,
            length,
        }
    }
}

impl Iterator for ElementChildIter {
    type Item = Element;

    fn next(&mut self) -> Option<Self::Item> {
        if self.index < self.length {
            let element = self.collection.item(self.index);
            self.index += 1;
            element
        } else {
            None
        }
    }
}
