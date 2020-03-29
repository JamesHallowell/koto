use crate::{
    builtin_value::BuiltinValue, value_list::ValueList, value_map::ValueMap, Runtime, RuntimeResult,
};
use koto_parser::{vec4, AstFor, AstWhile, Function};
use std::{cell::RefCell, cmp::Ordering, fmt, ops::Deref, rc::Rc};

#[derive(Clone, Debug)]
pub enum Value<'a> {
    Empty,
    Bool(bool),
    Number(f64),
    Vec4(vec4::Vec4),
    Range { start: isize, end: isize },
    IndexRange { start: usize, end: Option<usize> },
    List(Rc<RefCell<ValueList<'a>>>),
    Map(Rc<RefCell<ValueMap<'a>>>),
    Str(Rc<String>),
    Share(Rc<RefCell<Value<'a>>>),
    Function(Rc<Function>),
    BuiltinFunction(BuiltinFunction<'a>),
    BuiltinValue(Rc<RefCell<dyn BuiltinValue>>),
    For(Rc<AstFor>),
    While(Rc<AstWhile>),
}

impl<'a> fmt::Display for Value<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use Value::*;
        match self {
            Empty => f.write_str("()"),
            Bool(b) => f.write_str(&b.to_string()),
            Number(n) => f.write_str(&n.to_string()),
            Vec4(v) => write!(f, "({}, {}, {}, {})", v.0, v.1, v.2, v.3),
            Str(s) => f.write_str(&s),
            List(l) => f.write_str(&l.borrow().to_string()),
            Map(m) => {
                write!(f, "Map {:?} ", Rc::into_raw(m.clone()))?;
                write!(f, "{{")?;
                let mut first = true;
                for (key, _value) in m.borrow().0.iter() {
                    if first {
                        write!(f, " ")?;
                    } else {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", key)?;
                    first = false;
                }
                write!(f, " }}")
            }
            Range { start, end } => write!(f, "[{}..{}]", start, end),
            IndexRange { start, end } => write!(
                f,
                "[{}..{}]",
                start,
                end.map_or("".to_string(), |n| n.to_string()),
            ),
            Share(r) => {
                let value = deref_value(&r.borrow());
                write!(f, "Share: {}", value)
            }
            Function(function) => {
                let raw = Rc::into_raw(function.clone());
                write!(f, "Function: {:?}", raw)
            }
            BuiltinFunction(function) => {
                let raw = Rc::into_raw(function.function.clone());
                write!(f, "Builtin function: {:?}", raw)
            }
            BuiltinValue(ref value) => f.write_str(&value.borrow().to_string()),
            For(_) => write!(f, "For loop"),
            While(_) => write!(f, "While loop"),
        }
    }
}

impl<'a> PartialEq for Value<'a> {
    fn eq(&self, other: &Self) -> bool {
        use Value::*;

        match (self, other) {
            (Number(a), Number(b)) => a == b,
            (Vec4(a), Vec4(b)) => a == b,
            (Bool(a), Bool(b)) => a == b,
            (Str(a), Str(b)) => a.as_ref() == b.as_ref(),
            (List(a), List(b)) => a.as_ref() == b.as_ref(),
            (Map(a), Map(b)) => a.as_ref() == b.as_ref(),
            (Share(a), Share(b)) => a.as_ref() == b.as_ref(),
            (Share(a), _) => a.borrow().deref() == other,
            (_, Share(b)) => self == b.borrow().deref(),
            (Function(a), Function(b)) => Rc::ptr_eq(a, b),
            (Empty, Empty) => true,
            _ => false,
        }
    }
}
impl<'a> Eq for Value<'a> {}

impl<'a> PartialOrd for Value<'a> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        use Value::*;

        match (self, other) {
            (Number(a), Number(b)) => a.partial_cmp(b),
            (Vec4(a), Vec4(b)) => a.partial_cmp(b),
            (Str(a), Str(b)) => a.partial_cmp(b),
            (a, b) => panic!(format!("partial_cmp unsupported for {} and {}", a, b)),
        }
    }
}

impl<'a> Ord for Value<'a> {
    fn cmp(&self, other: &Self) -> Ordering {
        use Value::*;

        match (self, other) {
            (Number(a), Number(b)) => match (a.is_nan(), b.is_nan()) {
                (true, true) => Ordering::Equal,
                (false, true) => Ordering::Less,
                (true, false) => Ordering::Greater,
                (false, false) => a.partial_cmp(b).unwrap(),
            },
            (Str(a), Str(b)) => a.cmp(b),
            (a, b) => panic!(format!("cmp unsupported for {} and {}", a, b)),
        }
    }
}

impl<'a> From<bool> for Value<'a> {
    fn from(value: bool) -> Self {
        Self::Bool(value)
    }
}

// Once Trait aliases are stabilized this can be simplified a bit,
// see: https://github.com/rust-lang/rust/issues/55628
#[allow(clippy::type_complexity)]
pub struct BuiltinFunction<'a> {
    pub function: Rc<RefCell<dyn FnMut(&mut Runtime<'a>, &[Value<'a>]) -> RuntimeResult<'a> + 'a>>,
    pub is_instance_function: bool,
}

impl<'a> BuiltinFunction<'a> {
    pub fn new(
        function: impl FnMut(&mut Runtime<'a>, &[Value<'a>]) -> RuntimeResult<'a> + 'a,
        is_instance_function: bool,
    ) -> Self {
        Self {
            function: Rc::new(RefCell::new(function)),
            is_instance_function,
        }
    }
}

impl<'a> Clone for BuiltinFunction<'a> {
    fn clone(&self) -> Self {
        Self {
            function: self.function.clone(),
            is_instance_function: self.is_instance_function,
        }
    }
}

impl<'a> fmt::Debug for BuiltinFunction<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let raw = Rc::into_raw(self.function.clone());
        write!(
            f,
            "builtin {}function: {:?}",
            if self.is_instance_function {
                "instance "
            } else {
                ""
            },
            raw
        )
    }
}

pub fn values_have_matching_type<'a>(a: &Value<'a>, b: &Value<'a>) -> bool {
    use std::mem::discriminant;
    use Value::Share;

    match (a, b) {
        (Share(a), Share(b)) => discriminant(a.borrow().deref()) == discriminant(b.borrow().deref()),
        (Share(a), _) => discriminant(a.borrow().deref()) == discriminant(b),
        (_, Share(b)) => discriminant(a) == discriminant(b.borrow().deref()),
        (_, _) => discriminant(a) == discriminant(b),
    }
}

pub fn copy_value<'a>(value: &Value<'a>) -> Value<'a> {
    use Value::{List, Map, Share};

    match value {
        List(l) => List(Rc::new(RefCell::new(l.borrow().clone()))),
        Map(m) => Map(Rc::new(RefCell::new(m.borrow().clone()))),
        Share(r) => r.borrow().clone(),
        _ => value.clone(),
    }
}

pub fn deref_value<'a>(value: &Value<'a>) -> Value<'a> {
    use Value::Share;

    match value {
        Share(r) => r.borrow().clone(),
        _ => value.clone(),
    }
}

pub fn make_reference(mut value: Value) -> (Value, bool) {
    // When making a reference out of a List or Map, we need to call make_mut to ensure that
    // existing bindings to the internal data don't get modified by modifications to the reference
    match &mut value {
        Value::List(l) => {
            Rc::make_mut(l);
        }
        Value::Map(m) => {
            Rc::make_mut(m);
        }
        Value::Share(_) => {
            return (value, false);
        }
        _ => {}
    }
    let cloned = Rc::new(RefCell::new(value.clone()));
    (Value::Share(cloned), true)
}

pub fn type_as_string(value: &Value) -> String {
    use Value::*;
    match &value {
        Empty => "Empty".to_string(),
        Bool(_) => "Bool".to_string(),
        Number(_) => "Number".to_string(),
        Vec4(_) => "Vec4".to_string(),
        List(_) => "List".to_string(),
        Range { .. } => "Range".to_string(),
        IndexRange { .. } => "IndexRange".to_string(),
        Map(_) => "Map".to_string(),
        Str(_) => "String".to_string(),
        Share(value) => format!("Share({})", type_as_string(&deref_value(&value.borrow()))),
        Function(_) => "Function".to_string(),
        BuiltinFunction(_) => "BuiltinFunction".to_string(),
        BuiltinValue(value) => value.borrow().value_type(),
        For(_) => "For".to_string(),
        While(_) => "While".to_string(),
    }
}