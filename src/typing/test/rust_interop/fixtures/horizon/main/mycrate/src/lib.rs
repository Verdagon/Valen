


use std::collections::HashMap;
use std::ops::Deref;

pub fn add_two_numbers(a: i32, b: i32) -> i32 {
    a + b
}

#[track_caller]
pub fn tracked_sum(a: i32, b: i32) -> i32 {
    a + b
}

pub fn add_i64(a: i64, b: i64) -> i64 {
    a + b
}

pub struct Delta {
    secs: i64,
    nanos: i32,
}

impl Delta {
    pub fn seconds(secs: i64) -> Delta {
        Delta { secs, nanos: 0 }
    }

    pub fn num_seconds(&self) -> i64 {
        self.secs
    }
}

pub struct Counter {
    pub value: i32,
}

impl Counter {
    pub fn get(self) -> i32 {
        self.value
    }

    pub fn doubled(self) -> i32 {
        self.value * 2
    }

    ///
    pub fn or_else<T>(self, fallback: T) -> T {
        fallback
    }

    ///
    pub fn new() -> Counter {
        Counter { value: 5 }
    }

    pub fn peek(&self) -> i32 {
        self.value
    }
}

pub fn nudge(a: &mut Counter, b: &Counter) -> i32 {
    a.value = b.value;
    a.value
}

pub fn tie<'a>(a: &'a mut Counter, b: &'a mut Counter) -> i32 {
    a.value = b.value;
    a.value
}

pub struct Slot {
    value: Box<i64>,
}

impl Slot {
    pub fn new() -> Slot {
        Slot { value: Box::new(0) }
    }

    pub fn mutate(&mut self, x: i64) {
        *self.value = x;
    }

    pub fn get(&self) -> i64 {
        *self.value
    }
}

pub mod instruments {
    pub fn depth_reading() -> i32 {
        31
    }

    pub struct Sonar {
        pub depth: i32,
    }

    impl Sonar {
        pub fn depth_of(self) -> i32 {
            self.depth
        }
    }

    pub fn make_sonar() -> Sonar {
        Sonar { depth: 33 }
    }
}

pub mod readouts {
    pub use crate::instruments::depth_reading;
    pub use crate::instruments::make_sonar;
    pub use crate::instruments::Sonar;
}

pub mod gear {
    pub use crate::instruments;
}

pub struct Gauge {
    pub reading: i32,
}

impl Gauge {
    pub fn get(self) -> i32 {
        self.reading
    }
}

pub fn make_gauge() -> Gauge {
    Gauge { reading: 20 }
}

pub fn gauge_reading(g: Gauge) -> i32 {
    g.reading + 2
}

pub fn value_of_counter(c: Counter) -> i32 {
    c.value
}

pub fn bump(c: Counter) -> Counter {
    Counter { value: c.value + 1 }
}

pub fn seven() -> i32 {
    7
}

pub fn do_nothing() {}

pub fn is_positive(x: i32) -> bool {
    x > 0
}

pub fn to_int(b: bool) -> i32 {
    if b {
        1
    } else {
        0
    }
}

pub fn pick_second<A, B>(_a: A, b: B) -> B {
    b
}

pub fn id<T>(x: T) -> T {
    x
}

pub fn make_counter() -> Counter {
    Counter { value: 7 }
}

pub fn pick<A, B>(a: A, _b: B) -> A {
    a
}

pub struct Holder<T> {
    pub value: T,
}

impl<T> Holder<T> {
    pub fn into_value(self) -> T {
        self.value
    }
}

pub fn make_holder() -> Holder<i32> {
    Holder { value: 9 }
}

pub fn make_bool_holder() -> Holder<bool> {
    Holder { value: true }
}

pub fn holder_value(h: Holder<i32>) -> i32 {
    h.value
}

pub fn holder_ignore<T>(_h: Holder<T>) -> i32 {
    9
}

pub struct Fixed;

pub struct Boxed<T, A> {
    pub value: Option<T>,
    pub alloc: Option<A>,
}

impl<T> Boxed<T, Fixed> {
    pub fn new() -> Boxed<T, Fixed> {
        Boxed { value: None, alloc: None }
    }
}

pub fn boxed_ignore<T>(_b: Boxed<T, Fixed>) -> i32 {
    7
}

pub fn some_size() -> usize {
    3
}

pub fn consume_usize(_n: usize) -> i32 {
    8
}

pub enum Shade {
    Dim,
    Bright,
}

impl Shade {
    pub fn level(self) -> i32 {
        match self {
            Shade::Dim => 1,
            Shade::Bright => 2,
        }
    }
}

pub fn make_shade() -> Shade {
    Shade::Bright
}

pub fn bool_holder_flag(h: Holder<bool>) -> i32 {
    if h.value {
        1
    } else {
        0
    }
}

pub fn first<I: Iterator>(mut i: I) -> I::Item {
    i.next().unwrap()
}

pub fn take_first<I: Iterator>(_i: I, _x: I::Item) {}

pub fn unsigned_count() -> u32 {
    7
}

pub fn half_of(x: f32) -> f32 {
    x / 2.0
}

pub struct Hidden {
    pub magnitude: i32,
}

pub fn takes_hidden(h: Hidden) -> i32 {
    h.magnitude
}

pub struct Glyph {
    location: i32,
}

impl Glyph {
    pub fn new(location: i32) -> Glyph {
        Glyph { location }
    }

    pub fn location(&self) -> i32 {
        self.location
    }
}

pub struct Domino {
    glyphs: HashMap<i32, Glyph>,
}

impl Domino {
    pub fn new() -> Domino {
        Domino { glyphs: HashMap::new() }
    }

    pub fn add_glyph(&mut self, g: Glyph) -> i32 {
        let key = g.location;
        self.glyphs.insert(key, g);
        key
    }

    pub fn get_glyph(&self, key: i32) -> &Glyph {
        self.glyphs.get(&key).unwrap()
    }
}

pub fn domino_size(d: Domino) -> i32 {
    d.glyphs.len() as i32
}

pub fn add_and_return(mut d: Domino, loc: i32) -> Domino {
    d.glyphs.insert(loc, Glyph::new(loc));
    d
}

#[repr(C)]
pub struct Small8 {
    a: i32,
    b: i16,
    c: i16,
}

impl Small8 {
    pub fn small_a(&self) -> i32 {
        self.a
    }
}

pub fn make_small(a: i32, b: i32, c: i32) -> Small8 {
    Small8 { a, b: b as i16, c: c as i16 }
}

pub fn small_plus(s: Small8, bonus: i32) -> i32 {
    s.a + bonus
}

pub trait Callback {
    fn on_call(&self) -> i32;
}

pub struct Core {
    val: i32,
}

impl Core {
    pub fn read(&self) -> i32 {
        self.val
    }
}

pub struct Sheath {
    inner: Core,
}

impl Deref for Sheath {
    type Target = Core;

    fn deref(&self) -> &Core {
        &self.inner
    }
}

pub fn make_sheath() -> Sheath {
    Sheath { inner: Core { val: 7 } }
}
