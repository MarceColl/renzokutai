use crate::config::{Filter, Value};

pub trait Filterable {
    fn filter(&self, filter: &Option<Filter>) -> bool {
        match filter {
            None => true,
            Some(filter) => self.inner_filter(filter),
        }
    }

    fn inner_filter(&self, filter: &Filter) -> bool;
}

impl Filterable for crate::config::Repo {
    fn inner_filter(&self, filter: &Filter) -> bool {
        match filter.key.as_str() {
            "url" => self.url == Value::Set(filter.value.clone()),
            _ => false,
        }
    }
}

impl Filterable for crate::config::Package {
    fn inner_filter(&self, filter: &Filter) -> bool {
        match filter.key.as_str() {
            "name" => self.name == Value::Set(filter.value.clone()),
            "provider" => self.provider == Value::Set(filter.value.clone()),
            _ => false,
        }
    }
}

impl Filterable for crate::config::Step {
    fn inner_filter(&self, filter: &Filter) -> bool {
        match filter.key.as_str() {
            "name" => self.name == Value::Set(filter.value.clone()),
            "script" => self.script == Value::Set(filter.value.clone()),
            // "depends" => self.depends == filter.value,
            _ => false,
        }
    }
}
