use globin::InternedAsSlice;

fn main() {
    let a = InternedAsSlice::from_owned([1, 2, 3]).unsize();
    let b = InternedAsSlice::from_owned([1, 2]).unsize();

    assert_eq!(a, b);
}
