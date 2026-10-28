//! The fixture of the D-RL1 reading-list tests: the serve fixture with a reading
//! list on its one course.

#![allow(clippy::unwrap_used, dead_code)]

use cadus_core::curriculum::{Curriculum, RawCurriculum, ReadingEntry};

use crate::common::{
    DRILL_TEXT, EXEMPLAR_TEXT_2, EXPECTED_ANSWER, PROBLEM_TEXT, drill_topic, exemplar, kp,
    one_course_catalog, topic, unit,
};

/// The two entries of the fixture list: one address and one chapter note.
pub fn entries() -> Vec<ReadingEntry> {
    vec![
        ReadingEntry {
            title: "Free Primer".to_owned(),
            author: "A. Writer".to_owned(),
            access: "free".to_owned(),
            where_to_read: "https://example.org/primer".to_owned(),
            note: "Short pages with examples.".to_owned(),
        },
        ReadingEntry {
            title: "Paper Book".to_owned(),
            author: "B. Writer".to_owned(),
            access: "book".to_owned(),
            where_to_read: "Chapter 2, Fractions".to_owned(),
            note: "Practice problems at the back.".to_owned(),
        },
    ]
}

/// The serve fixture of course `c1` ("Foundations"), with
/// `reading` as the list of the course.
pub fn curriculum(reading: Vec<ReadingEntry>) -> Curriculum {
    curriculum_with_term(reading, None)
}

/// [`curriculum`] with `lt_term` as the Lithuanian term of the topic `addition` (D-VBE1).
pub fn curriculum_with_term(reading: Vec<ReadingEntry>, lt_term: Option<&str>) -> Curriculum {
    let mut catalog = one_course_catalog();
    catalog.courses[0].reading = reading;
    let mut topics = vec![
        topic(
            "addition",
            vec![
                kp(
                    "kp1",
                    vec![
                        exemplar(PROBLEM_TEXT, EXPECTED_ANSWER),
                        exemplar(EXEMPLAR_TEXT_2, "13.25"),
                    ],
                ),
                kp("kp2", vec![exemplar("Compute 40 + 2.5.", "42.5")]),
            ],
        ),
        topic("subtraction", vec![kp("kp1", vec![])]),
        drill_topic(
            "tables",
            vec![kp(
                "kp1",
                vec![
                    exemplar(DRILL_TEXT, "56.5"),
                    exemplar("Compute 6 x 7 + 0.25.", "42.25"),
                ],
            )],
        ),
    ];
    topics[0].lt_term = lt_term.map(str::to_owned);
    Curriculum::build(RawCurriculum {
        catalog,
        units: vec![unit("c1", "M1", topics, 0)],
    })
    .unwrap()
}
