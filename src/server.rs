use std::{collections::HashMap, sync::Arc};

use crate::{book::library::Library, teacher::TeacherAgent};

type StudentBookId = (i64, i64);

pub struct Server {
    library: Arc<Library>,
    teacher_agents: HashMap<StudentBookId, TeacherAgent>,
}

impl Server {
    pub fn new(library: Arc<Library>) -> Self {
        Self {
            library,
            teacher_agents: HashMap::new(),
        }
    }
}
