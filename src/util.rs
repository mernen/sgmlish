use std::borrow::Cow;

pub(crate) fn make_owned<T: ?Sized + ToOwned>(cow: Cow<T>) -> Cow<'static, T> {
    match cow {
        Cow::Borrowed(x) => Cow::Owned(x.to_owned()),
        Cow::Owned(x) => Cow::Owned(x),
    }
}
