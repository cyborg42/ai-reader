-- Add migration script here
CREATE TABLE book (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    title TEXT NOT NULL,
    author TEXT NOT NULL,
    path TEXT NOT NULL UNIQUE,
    description TEXT,
    summary TEXT
);

CREATE TABLE student (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL
);

CREATE TABLE study_plan (
    book_id INTEGER,
    student_id INTEGER,
    plan TEXT NOT NULL,
    PRIMARY KEY (book_id, student_id),
    FOREIGN KEY (book_id) REFERENCES book(id) ON DELETE CASCADE,
    FOREIGN KEY (student_id) REFERENCES student(id) ON DELETE CASCADE
);

CREATE TABLE chapter (
    book_id INTEGER,
    index_number CHAR(20) NOT NULL,
    name TEXT NOT NULL,
    summary TEXT,
    PRIMARY KEY (book_id, index_number),
    FOREIGN KEY (book_id) REFERENCES book(id) ON DELETE CASCADE
);

CREATE TABLE learning_progress (
    student_id INTEGER,
    book_id INTEGER,
    chapter_index CHAR(20) NOT NULL,
    status TINYINT CHECK(status BETWEEN 0 AND 2),
    description TEXT,
    update_time DATETIME DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (student_id, book_id, chapter_index),
    FOREIGN KEY (student_id) REFERENCES student(id) ON DELETE CASCADE,
    FOREIGN KEY (book_id) REFERENCES book(id) ON DELETE CASCADE,
    FOREIGN KEY (chapter_index) REFERENCES chapter(index_number) ON DELETE CASCADE
);
