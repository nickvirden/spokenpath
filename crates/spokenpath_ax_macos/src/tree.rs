//! The walk of the tree under the web area and the encoding of every attribute value, written
//! against [`Source`] so the same logic runs over the macOS accessibility API and over plain test
//! trees on every platform.

use std::collections::HashMap;

use spokenpath_snapshot::{SafariAxNode, SafariAxValue};

use crate::failure::{Code, Failure};

/// The most nodes a read records; a larger tree fails rather than growing without limit.
pub(crate) const MAX_NODES: usize = 50_000;

/// The attribute whose elements are a node's children.
pub(crate) const CHILDREN: &str = "AXChildren";

/// A value as it was read, before element references are resolved to node ids.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Raw<E> {
    /// A `CFString`, as its UTF-16 code units.
    String(Vec<u16>),
    Boolean(bool),
    Number(NumberValue),
    /// A `CFURL`, as the UTF-16 code units of its absolute string.
    Url(Vec<u16>),
    Array(Vec<Raw<E>>),
    Element(E),
    Point {
        x: f64,
        y: f64,
    },
    Size {
        width: f64,
        height: f64,
    },
    Rect {
        x: f64,
        y: f64,
        width: f64,
        height: f64,
    },
    Range {
        location: i64,
        length: i64,
    },
    /// Any other type, by the UTF-16 code units of its type's description.
    Unsupported(Vec<u16>),
    /// The `AXError` the read returned.
    Error(i32),
}

/// A `CFNumber`, kept in the form Safari delivered it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum NumberValue {
    Integer(i64),
    Float(f64),
}

/// What the walk reads from: one accessibility element type and its calls.
pub(crate) trait Source {
    type Element;

    /// The names `AXUIElementCopyAttributeNames` returns, or the `AXError` it failed with.
    fn attribute_names(&mut self, element: &Self::Element) -> Result<Vec<String>, i32>;

    /// One attribute's value, or `Raw::Error` with the code the read failed with.
    fn attribute(&mut self, element: &Self::Element, name: &str) -> Raw<Self::Element>;

    /// Whether two elements are the same element (`CFEqual`).
    fn same(&self, left: &Self::Element, right: &Self::Element) -> bool;

    /// A hash equal for elements that are the same (`CFHash`).
    fn hash(&self, element: &Self::Element) -> u64;

    /// Fails once the read's deadline has passed.
    ///
    /// # Errors
    ///
    /// A `timeout` failure.
    fn check_deadline(&self) -> Result<(), Failure>;
}

/// A recorded node before its element references are resolved.
struct Walked<E> {
    parent: Option<u32>,
    children: Vec<u32>,
    attributes: Vec<(String, Raw<E>)>,
}

/// The recorded elements, looked up by `CFHash` and confirmed by `CFEqual`.
struct Recorded<E> {
    elements: Vec<E>,
    by_hash: HashMap<u64, Vec<u32>>,
}

impl<E> Recorded<E> {
    fn find<S: Source<Element = E>>(&self, source: &S, element: &E) -> Option<u32> {
        self.by_hash
            .get(&source.hash(element))?
            .iter()
            .copied()
            .find(|&id| {
                usize::try_from(id)
                    .ok()
                    .and_then(|index| self.elements.get(index))
                    .is_some_and(|recorded| source.same(recorded, element))
            })
    }
}

/// Walks the tree under `root` in pre-order through `AXChildren`, in Safari's order, recording
/// every attribute of every node, and returns the nodes with each element reference resolved to
/// the id of the recorded node that is the same element. An element met again is not walked
/// again. The recorded elements are returned with the nodes, in id order.
///
/// # Errors
///
/// `timeout` once the deadline passes, `ax-tree-too-large` beyond [`MAX_NODES`] nodes, and
/// `ax-read-failed` when a node's attribute names cannot be read.
pub(crate) fn walk<S: Source>(
    source: &mut S,
    root: S::Element,
) -> Result<(Vec<SafariAxNode>, Vec<S::Element>), Failure>
where
    S::Element: Clone,
{
    let mut walked: Vec<Walked<S::Element>> = Vec::new();
    let mut recorded = Recorded {
        elements: Vec::new(),
        by_hash: HashMap::new(),
    };
    // Children are pushed last first, so they are popped, and numbered, in their order.
    let mut pending: Vec<(S::Element, Option<u32>)> = vec![(root, None)];
    while let Some((element, parent)) = pending.pop() {
        source.check_deadline()?;
        if recorded.find(source, &element).is_some() {
            continue;
        }
        if walked.len() >= MAX_NODES {
            return Err(Failure::new(
                Code::AxTreeTooLarge,
                format!(
                    "Safari's macOS accessibility tree for the page has more than {MAX_NODES} nodes, the most a capture records"
                ),
            ));
        }
        let id = u32::try_from(walked.len()).expect("MAX_NODES fits in a u32");
        if let Some(parent) = parent {
            walked[usize::try_from(parent).expect("a node id is an index")]
                .children
                .push(id);
        }
        let names = source.attribute_names(&element).map_err(|error| {
            Failure::new(
                Code::AxReadFailed,
                format!(
                    "the attribute names of node {id} of Safari's macOS accessibility tree cannot be read (AXError {error}), as when the page changes while it is read; run the capture again"
                ),
            )
        })?;
        let mut attributes = Vec::with_capacity(names.len());
        for name in names {
            source.check_deadline()?;
            let value = source.attribute(&element, &name);
            attributes.push((name, value));
        }
        let children: Vec<S::Element> = match attributes.iter().find(|(name, _)| name == CHILDREN) {
            Some((_, Raw::Array(items))) => items
                .iter()
                .filter_map(|item| match item {
                    Raw::Element(child) => Some(child.clone()),
                    _ => None,
                })
                .collect(),
            _ => Vec::new(),
        };
        pending.extend(children.into_iter().rev().map(|child| (child, Some(id))));
        recorded
            .by_hash
            .entry(source.hash(&element))
            .or_default()
            .push(id);
        recorded.elements.push(element);
        walked.push(Walked {
            parent,
            children: Vec::new(),
            attributes,
        });
    }
    let nodes = walked
        .into_iter()
        .enumerate()
        .map(|(index, node)| SafariAxNode {
            id: u32::try_from(index).expect("MAX_NODES fits in a u32"),
            parent: node.parent,
            children: node.children,
            span: None,
            attributes: node
                .attributes
                .into_iter()
                .map(|(name, value)| {
                    let value = encode(value, &|element| recorded.find(source, element));
                    (name, value)
                })
                .collect(),
        })
        .collect();
    Ok((nodes, recorded.elements))
}

/// The snapshot value of a raw value, with each element reference resolved by `resolve`.
pub(crate) fn encode<E>(raw: Raw<E>, resolve: &impl Fn(&E) -> Option<u32>) -> SafariAxValue {
    match raw {
        Raw::String(units) => SafariAxValue::String {
            value: String::from_utf16_lossy(&units),
        },
        Raw::Boolean(value) => SafariAxValue::Boolean { value },
        Raw::Number(NumberValue::Integer(value)) => SafariAxValue::Number {
            value: value.into(),
        },
        // JSON has no NaN or infinity, so such a number is recorded by its type alone.
        Raw::Number(NumberValue::Float(value)) => match serde_json::Number::from_f64(value) {
            Some(value) => SafariAxValue::Number { value },
            None => SafariAxValue::Unsupported {
                cf_type: "CFNumber".to_owned(),
            },
        },
        Raw::Url(units) => SafariAxValue::Url {
            value: String::from_utf16_lossy(&units),
        },
        Raw::Array(items) => SafariAxValue::Array {
            items: items
                .into_iter()
                .map(|item| encode(item, resolve))
                .collect(),
        },
        Raw::Element(element) => SafariAxValue::Element {
            node: resolve(&element),
        },
        Raw::Point { x, y } => SafariAxValue::Point { x, y },
        Raw::Size { width, height } => SafariAxValue::Size { width, height },
        Raw::Rect {
            x,
            y,
            width,
            height,
        } => SafariAxValue::Rect {
            x,
            y,
            width,
            height,
        },
        Raw::Range { location, length } => SafariAxValue::Range { location, length },
        Raw::Unsupported(units) => SafariAxValue::Unsupported {
            cf_type: String::from_utf16_lossy(&units),
        },
        Raw::Error(code) => SafariAxValue::Error { code },
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use spokenpath_snapshot::SafariAxValue;

    use super::{MAX_NODES, NumberValue, Raw, Source, encode, walk};
    use crate::failure::{Code, Failure};

    fn utf16(text: &str) -> Vec<u16> {
        text.encode_utf16().collect()
    }

    /// A plain tree: element `n` is `nodes[n]`, a list of attributes. Element `99` is outside it.
    struct Plain {
        nodes: Vec<Vec<(&'static str, Raw<usize>)>>,
        reads_left: Cell<usize>,
    }

    impl Plain {
        fn new(nodes: Vec<Vec<(&'static str, Raw<usize>)>>) -> Self {
            Plain {
                nodes,
                reads_left: Cell::new(usize::MAX),
            }
        }
    }

    impl Source for Plain {
        type Element = usize;

        fn attribute_names(&mut self, element: &usize) -> Result<Vec<String>, i32> {
            self.nodes
                .get(*element)
                .map(|attributes| {
                    attributes
                        .iter()
                        .map(|(name, _)| (*name).to_owned())
                        .collect()
                })
                .ok_or(-25202)
        }

        fn attribute(&mut self, element: &usize, name: &str) -> Raw<usize> {
            self.reads_left.set(self.reads_left.get().saturating_sub(1));
            self.nodes[*element]
                .iter()
                .find(|(candidate, _)| *candidate == name)
                .map_or(Raw::Error(-25205), |(_, value)| value.clone())
        }

        fn same(&self, left: &usize, right: &usize) -> bool {
            left == right
        }

        fn hash(&self, element: &usize) -> u64 {
            // Two elements share each hash, so equality tells them apart.
            u64::try_from(*element / 2).expect("a test element fits")
        }

        fn check_deadline(&self) -> Result<(), Failure> {
            if self.reads_left.get() == 0 {
                Err(Failure::new(Code::Timeout, "out of time"))
            } else {
                Ok(())
            }
        }
    }

    fn children(ids: &[usize]) -> (&'static str, Raw<usize>) {
        (
            "AXChildren",
            Raw::Array(ids.iter().copied().map(Raw::Element).collect()),
        )
    }

    fn role(name: &str) -> (&'static str, Raw<usize>) {
        ("AXRole", Raw::String(utf16(name)))
    }

    #[test]
    fn walks_in_pre_order_and_numbers_nodes_in_that_order() {
        // 0 → [3, 1]; 3 → [2]; so pre-order is 0, 3, 2, 1.
        let mut tree = Plain::new(vec![
            vec![
                role("AXWebArea"),
                children(&[3, 1]),
                ("AXParent", Raw::Element(99)),
            ],
            vec![role("AXButton"), ("AXParent", Raw::Element(0))],
            vec![role("AXStaticText"), ("AXParent", Raw::Element(3))],
            vec![
                role("AXHeading"),
                children(&[2]),
                ("AXParent", Raw::Element(0)),
            ],
        ]);
        let (nodes, elements) = walk(&mut tree, 0).expect("the walk succeeds");
        assert_eq!(elements, vec![0, 3, 2, 1]);
        let shape: Vec<(u32, Option<u32>, Vec<u32>)> = nodes
            .iter()
            .map(|node| (node.id, node.parent, node.children.clone()))
            .collect();
        assert_eq!(
            shape,
            vec![
                (0, None, vec![1, 3]),
                (1, Some(0), vec![2]),
                (2, Some(1), vec![]),
                (3, Some(0), vec![])
            ]
        );
        assert_eq!(
            nodes[0].attributes["AXParent"],
            SafariAxValue::Element { node: None },
            "an element outside the tree records nothing else"
        );
        assert_eq!(
            nodes[2].attributes["AXParent"],
            SafariAxValue::Element { node: Some(1) }
        );
        assert_eq!(
            nodes[0].attributes["AXChildren"],
            SafariAxValue::Array {
                items: vec![
                    SafariAxValue::Element { node: Some(1) },
                    SafariAxValue::Element { node: Some(3) }
                ]
            }
        );
    }

    #[test]
    fn an_element_met_again_is_recorded_only_as_a_reference() {
        // 2 is a child of both 0 and 1, and 1 lists 0 again.
        let mut tree = Plain::new(vec![
            vec![children(&[1, 2])],
            vec![children(&[2, 0])],
            vec![role("AXGroup")],
        ]);
        let (nodes, _) = walk(&mut tree, 0).expect("the walk succeeds");
        assert_eq!(nodes.len(), 3);
        assert_eq!(nodes[0].children, vec![1]);
        assert_eq!(nodes[1].children, vec![2]);
        assert_eq!(
            nodes[0].attributes["AXChildren"],
            SafariAxValue::Array {
                items: vec![
                    SafariAxValue::Element { node: Some(1) },
                    SafariAxValue::Element { node: Some(2) }
                ]
            }
        );
        assert_eq!(
            nodes[1].attributes["AXChildren"],
            SafariAxValue::Array {
                items: vec![
                    SafariAxValue::Element { node: Some(2) },
                    SafariAxValue::Element { node: Some(0) }
                ]
            }
        );
    }

    #[test]
    fn records_every_attribute_name_with_its_value_or_its_error() {
        let mut tree = Plain::new(vec![vec![
            role("AXWebArea"),
            ("AXTitle", Raw::Error(-25212)),
            ("AXFocused", Raw::Boolean(true)),
        ]]);
        let (nodes, _) = walk(&mut tree, 0).expect("the walk succeeds");
        let names: Vec<&str> = nodes[0].attributes.keys().map(String::as_str).collect();
        assert_eq!(names, vec!["AXFocused", "AXRole", "AXTitle"]);
        assert_eq!(
            nodes[0].attributes["AXTitle"],
            SafariAxValue::Error { code: -25212 }
        );
    }

    #[test]
    fn fails_beyond_the_node_limit() {
        let mut tree = Plain::new(
            (0..=MAX_NODES)
                .map(|index| {
                    if index < MAX_NODES {
                        vec![children(&[index + 1])]
                    } else {
                        vec![role("AXGroup")]
                    }
                })
                .collect(),
        );
        let failure = walk(&mut tree, 0).expect_err("the tree is too large");
        assert_eq!(failure.code, Code::AxTreeTooLarge);
        assert!(failure.message.contains("50000"), "{}", failure.message);
    }

    #[test]
    fn fails_when_the_deadline_passes_between_reads() {
        let mut tree = Plain::new(vec![vec![children(&[1])], vec![role("AXGroup")]]);
        tree.reads_left.set(1);
        let failure = walk(&mut tree, 0).expect_err("the deadline passes");
        assert_eq!(failure.code, Code::Timeout);
    }

    #[test]
    fn fails_when_attribute_names_cannot_be_read() {
        let mut tree = Plain::new(vec![vec![children(&[5])]]);
        let failure = walk(&mut tree, 0).expect_err("element 5 does not exist");
        assert_eq!(failure.code, Code::AxReadFailed);
        assert!(
            failure.message.contains("AXError -25202"),
            "{}",
            failure.message
        );
    }

    #[test]
    fn encodes_every_value_type() {
        let resolve = |element: &usize| (*element == 7).then_some(3);
        let cases: Vec<(Raw<usize>, SafariAxValue)> = vec![
            (
                Raw::String(utf16("Café")),
                SafariAxValue::String {
                    value: "Café".to_owned(),
                },
            ),
            (Raw::Boolean(false), SafariAxValue::Boolean { value: false }),
            (
                Raw::Number(NumberValue::Integer(-4)),
                SafariAxValue::Number { value: (-4).into() },
            ),
            (
                Raw::Number(NumberValue::Float(1.5)),
                SafariAxValue::Number {
                    value: serde_json::Number::from_f64(1.5).expect("finite"),
                },
            ),
            (
                Raw::Number(NumberValue::Float(f64::NAN)),
                SafariAxValue::Unsupported {
                    cf_type: "CFNumber".to_owned(),
                },
            ),
            (
                Raw::Url(utf16("http://127.0.0.1:1/x")),
                SafariAxValue::Url {
                    value: "http://127.0.0.1:1/x".to_owned(),
                },
            ),
            (
                Raw::Array(vec![Raw::Element(7), Raw::Element(8)]),
                SafariAxValue::Array {
                    items: vec![
                        SafariAxValue::Element { node: Some(3) },
                        SafariAxValue::Element { node: None },
                    ],
                },
            ),
            (
                Raw::Point { x: 1.0, y: -2.5 },
                SafariAxValue::Point { x: 1.0, y: -2.5 },
            ),
            (
                Raw::Size {
                    width: 3.0,
                    height: 4.0,
                },
                SafariAxValue::Size {
                    width: 3.0,
                    height: 4.0,
                },
            ),
            (
                Raw::Rect {
                    x: 1.0,
                    y: 2.0,
                    width: 3.0,
                    height: 4.0,
                },
                SafariAxValue::Rect {
                    x: 1.0,
                    y: 2.0,
                    width: 3.0,
                    height: 4.0,
                },
            ),
            (
                Raw::Range {
                    location: 0,
                    length: 9,
                },
                SafariAxValue::Range {
                    location: 0,
                    length: 9,
                },
            ),
            (
                Raw::Unsupported(utf16("AXTextMarker")),
                SafariAxValue::Unsupported {
                    cf_type: "AXTextMarker".to_owned(),
                },
            ),
            (Raw::Error(-25212), SafariAxValue::Error { code: -25212 }),
        ];
        for (raw, expected) in cases {
            assert_eq!(encode(raw.clone(), &resolve), expected, "{raw:?}");
        }
    }

    #[test]
    fn unpaired_surrogates_become_the_replacement_character() {
        let units = vec![0x0041, 0xd800, 0x0042, 0xdc00];
        assert_eq!(
            encode::<usize>(Raw::String(units), &|_| None),
            SafariAxValue::String {
                value: "A\u{fffd}B\u{fffd}".to_owned()
            }
        );
    }

    #[test]
    fn a_walked_tree_serialises_as_the_snapshot_nodes() {
        let mut tree = Plain::new(vec![vec![
            role("AXWebArea"),
            (
                "AXSize",
                Raw::Size {
                    width: 1.0,
                    height: 2.0,
                },
            ),
        ]]);
        let (nodes, _) = walk(&mut tree, 0).expect("the walk succeeds");
        let json = serde_json::to_value(&nodes).expect("serialises");
        assert_eq!(
            json,
            serde_json::json!([{
                "id": 0,
                "children": [],
                "attributes": {
                    "AXRole": { "type": "string", "value": "AXWebArea" },
                    "AXSize": { "type": "size", "width": 1.0, "height": 2.0 }
                }
            }])
        );
    }
}
