use std::mem;

use never::Never;

use crate::{Data, SgmlEvent, SgmlFragment};

use super::Transform;

/// The action to be taken on the event being processed.
/// Used as a return value for closures passed to [`SgmlFragment::map_data`].
#[derive(Debug, PartialEq)]
pub enum MapDataResult<'a, E> {
    /// Keep the event, with the given data.
    Use(Data<'a>),
    /// Remove the event from the stream.
    Remove,
    /// Interrupt the transform with an error.
    Err(E),
}

impl<'a, E> MapDataResult<'a, E> {
    fn map<T, F: FnOnce(Data<'a>) -> T>(self, f: F) -> Result<Option<T>, E> {
        match self {
            MapDataResult::Use(data) => Ok(Some(f(data))),
            MapDataResult::Remove => Ok(None),
            MapDataResult::Err(err) => Err(err),
        }
    }
}

impl<'a> From<Data<'a>> for MapDataResult<'a, Never> {
    fn from(data: Data<'a>) -> Self {
        MapDataResult::Use(data)
    }
}

impl<'a> From<Option<Data<'a>>> for MapDataResult<'a, Never> {
    fn from(value: Option<Data<'a>>) -> Self {
        match value {
            Some(data) => MapDataResult::Use(data),
            None => MapDataResult::Remove,
        }
    }
}

impl<'a, E> From<Result<Data<'a>, E>> for MapDataResult<'a, E> {
    fn from(value: Result<Data<'a>, E>) -> Self {
        match value {
            Ok(data) => MapDataResult::Use(data),
            Err(err) => MapDataResult::Err(err),
        }
    }
}

impl<'a, E> From<Result<Option<Data<'a>>, E>> for MapDataResult<'a, E> {
    fn from(value: Result<Option<Data<'a>>, E>) -> Self {
        match value {
            Ok(Some(data)) => MapDataResult::Use(data),
            Ok(None) => MapDataResult::Remove,
            Err(err) => MapDataResult::Err(err),
        }
    }
}

pub(crate) fn try_map_data<'a, F, R, E>(
    mut fragment: SgmlFragment<'a>,
    mut f: F,
) -> Result<SgmlFragment<'a>, E>
where
    F: FnMut(Data<'a>, Option<&str>) -> R,
    R: Into<MapDataResult<'a, E>>,
{
    let mut transform = Transform::new();

    for (i, slot) in fragment.iter_mut().enumerate() {
        let event = mem::replace(slot, SgmlEvent::XmlCloseEmptyElement);
        let result = match event {
            SgmlEvent::Character(data) => f(data, None).into().map(SgmlEvent::Character)?,
            SgmlEvent::Attribute(key, Some(value)) => f(value, Some(&key))
                .into()
                .map(|value| SgmlEvent::Attribute(key, Some(value)))?,
            event => Some(event),
        };
        match result {
            Some(event) => *slot = event,
            None => transform.remove_at(i),
        }
    }

    Ok(transform.apply(fragment))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_map() {
        let fragment = SgmlFragment::from(vec![
            SgmlEvent::Attribute("X".into(), Some(Data::RcData("value".into()))),
            SgmlEvent::Character(Data::RcData("rcdata".into())),
            SgmlEvent::Character(Data::CData("cdata".into())),
        ]);

        let result = try_map_data(fragment, |data, attr| {
            Data::CData(format!("{:?}={:?}", attr, data).into())
        })
        .unwrap()
        .into_vec();

        assert_eq!(result.len(), 3);
        assert_eq!(
            result[0],
            SgmlEvent::Attribute(
                "X".into(),
                Some(Data::CData(r##"Some("X")=RcData("value")"##.into()))
            )
        );
        assert_eq!(
            result[1],
            SgmlEvent::Character(Data::CData(r##"None=RcData("rcdata")"##.into()))
        );
        assert_eq!(
            result[2],
            SgmlEvent::Character(Data::CData(r##"None=CData("cdata")"##.into()))
        );
    }
}
