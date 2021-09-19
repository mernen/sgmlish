use crate::{SgmlEvent, SgmlFragment};

/// A convenience helper to insert and remove events from a [`SgmlFragment`].
#[derive(Clone, Debug, Default)]
pub struct Transform<'a> {
    insertions: Vec<(usize, SgmlEvent<'a>)>,
    deletions: Vec<usize>,
}

impl<'a> Transform<'a> {
    /// Creates a new empty `Transform`.
    pub fn new() -> Self {
        Default::default()
    }

    /// Returns `true` if no operations were recorded.
    pub fn is_empty(&self) -> bool {
        self.insertions.is_empty() && self.deletions.is_empty()
    }

    /// Removes the event at the given position.
    ///
    /// The position is always relative to the original list --- you should not
    /// adjust indices to acommodate other insertions or removals.
    ///
    /// Removing the same `index` multiple times is a no-op, since indices are not readjusted.
    ///
    /// # Example
    ///
    /// ```rust
    /// # use sgmlish::{SgmlEvent, SgmlFragment};
    /// # use sgmlish::transforms::Transform;
    /// let fragment = SgmlFragment::from(vec![
    ///     /* 0 */ SgmlEvent::OpenStartTag { name: "A".into() },
    ///     /* 1 */ SgmlEvent::Attribute("HREF".into(), Some("/".into())),
    ///     /* 2 */ SgmlEvent::CloseStartTag,
    ///     /* 3 */ SgmlEvent::Character("hello".into()),
    ///     /* 4 */ SgmlEvent::EndTag { name: "A".into() },
    ///     /* 5 */ SgmlEvent::Character("!".into()),
    ///     /* 6 */
    /// ]);
    ///
    /// let mut transform = Transform::new();
    /// // Remove the attribute
    /// transform.remove_at(1);
    /// // Remove the final exclamation mark
    /// transform.remove_at(5);
    /// let result = transform.apply(fragment);
    ///
    /// assert_eq!(
    ///     result.into_vec(),
    ///     vec![
    ///         SgmlEvent::OpenStartTag { name: "A".into() },
    ///         SgmlEvent::CloseStartTag,
    ///         SgmlEvent::Character("hello".into()),
    ///         SgmlEvent::EndTag { name: "A".into() },
    ///     ]
    /// );
    /// ```
    pub fn remove_at(&mut self, index: usize) {
        self.deletions.push(index);
    }

    /// Inserts an event at the given position.
    ///
    /// The position is always relative to the original list --- you should not
    /// adjust indices to acommodate other insertions or removals.
    ///
    /// Inserting multiple events at the same `index` will place them in the order they were passed.
    ///
    /// # Example
    ///
    /// ```rust
    /// # use sgmlish::{SgmlEvent, SgmlFragment};
    /// # use sgmlish::transforms::Transform;
    /// let fragment = SgmlFragment::from(vec![
    ///     /* 0 */ SgmlEvent::OpenStartTag { name: "A".into() },
    ///     /* 1 */ SgmlEvent::Attribute("HREF".into(), Some("/".into())),
    ///     /* 2 */ SgmlEvent::CloseStartTag,
    ///     /* 3 */ SgmlEvent::Character("hello".into()),
    ///     /* 4 */
    /// ]);
    ///
    /// let mut transform = Transform::new();
    /// // Insert another attribute
    /// transform.insert_at(2, SgmlEvent::Attribute("TARGET".into(), Some("_blank".into())));
    /// // Insert end tag
    /// transform.insert_at(4, SgmlEvent::EndTag { name: "A".into() });
    /// let result = transform.apply(fragment);
    ///
    /// assert_eq!(
    ///     result.into_vec(),
    ///     vec![
    ///         SgmlEvent::OpenStartTag { name: "A".into() },
    ///         SgmlEvent::Attribute("HREF".into(), Some("/".into())),
    ///         SgmlEvent::Attribute("TARGET".into(), Some("_blank".into())),
    ///         SgmlEvent::CloseStartTag,
    ///         SgmlEvent::Character("hello".into()),
    ///         SgmlEvent::EndTag { name: "A".into() },
    ///     ]
    /// );
    /// ```
    pub fn insert_at(&mut self, index: usize, event: SgmlEvent<'a>) {
        self.insertions.push((index, event));
    }

    /// Applies the recorded changes to the given fragment.
    pub fn apply(self, fragment: SgmlFragment<'a>) -> SgmlFragment<'a> {
        if self.is_empty() {
            return fragment;
        }

        let mut deletions = self.deletions;
        deletions.sort_unstable();
        deletions.dedup();
        let mut deletions = deletions.into_iter().peekable();

        let mut insertions = self.insertions;
        insertions.sort_by_key(|(pos, _)| *pos);
        let mut insertions = insertions.into_iter().peekable();

        let final_size = fragment.len().saturating_sub(deletions.len()) + insertions.len();
        let mut result = Vec::with_capacity(final_size);

        for (i, event) in fragment.into_iter().enumerate() {
            while let Some((_, event_to_insert)) =
                insertions.next_if(|(index_to_insert, _)| *index_to_insert == i)
            {
                result.push(event_to_insert);
            }

            if deletions.next_if_eq(&i).is_none() {
                result.push(event);
            }
        }

        // Insert remaining events at the end
        result.extend(insertions.map(|(_, event)| event));

        result.into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_insert_multiple_times_same_index() {
        let fragment = SgmlFragment::from(vec![
            SgmlEvent::OpenStartTag { name: "IMG".into() },
            SgmlEvent::Attribute("SRC".into(), Some("example.gif".into())),
            SgmlEvent::CloseStartTag,
        ]);

        let mut transform = Transform::new();
        transform.insert_at(2, SgmlEvent::Attribute("BORDER".into(), Some("0".into())));
        transform.insert_at(2, SgmlEvent::Attribute("ISMAP".into(), None));

        let result = transform.apply(fragment);

        assert_eq!(
            result,
            SgmlFragment::from(vec![
                SgmlEvent::OpenStartTag { name: "IMG".into() },
                SgmlEvent::Attribute("SRC".into(), Some("example.gif".into())),
                SgmlEvent::Attribute("BORDER".into(), Some("0".into())),
                SgmlEvent::Attribute("ISMAP".into(), None),
                SgmlEvent::CloseStartTag,
            ])
        );
    }

    #[test]
    fn test_remove_multiple_times_same_index() {
        let fragment = SgmlFragment::from(vec![
            SgmlEvent::OpenStartTag { name: "IMG".into() },
            SgmlEvent::Attribute("SRC".into(), Some("example.gif".into())),
            SgmlEvent::Attribute("BORDER".into(), Some("0".into())),
            SgmlEvent::Attribute("ISMAP".into(), None),
            SgmlEvent::CloseStartTag,
        ]);

        let mut transform = Transform::new();
        transform.remove_at(2);
        transform.remove_at(3);
        transform.remove_at(3);

        let result = transform.apply(fragment);

        assert_eq!(
            result,
            SgmlFragment::from(vec![
                SgmlEvent::OpenStartTag { name: "IMG".into() },
                SgmlEvent::Attribute("SRC".into(), Some("example.gif".into())),
                SgmlEvent::CloseStartTag,
            ])
        );
    }

    #[test]
    fn test_insert_remove_at_same_index() {
        let fragment = SgmlFragment::from(vec![
            SgmlEvent::OpenStartTag { name: "A".into() },
            SgmlEvent::Attribute("HREF".into(), Some("/".into())),
            SgmlEvent::CloseStartTag,
            SgmlEvent::Character("hello".into()),
            SgmlEvent::EndTag { name: "A".into() },
        ]);

        let mut transform = Transform::new();
        transform.insert_at(
            1,
            SgmlEvent::Attribute("NAME".into(), Some("greeting".into())),
        );
        transform.remove_at(1);
        let result = transform.apply(fragment);

        assert_eq!(
            result,
            SgmlFragment::from(vec![
                SgmlEvent::OpenStartTag { name: "A".into() },
                SgmlEvent::Attribute("NAME".into(), Some("greeting".into())),
                SgmlEvent::CloseStartTag,
                SgmlEvent::Character("hello".into()),
                SgmlEvent::EndTag { name: "A".into() },
            ])
        );
    }
}
