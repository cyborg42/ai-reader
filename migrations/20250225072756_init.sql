-- Add migration script here
-- Enable foreign keys
PRAGMA foreign_keys = ON;

CREATE TABLE book (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    title TEXT NOT NULL,
    path TEXT NOT NULL UNIQUE,
    summary TEXT NOT NULL,
    author TEXT,
    description TEXT
);

CREATE TABLE chapter (
    book_id INTEGER,
    chapter_number CHAR(20) NOT NULL,
    name TEXT NOT NULL,
    summary TEXT NOT NULL,
    PRIMARY KEY (book_id, chapter_number),
    FOREIGN KEY (book_id) REFERENCES book(id) ON DELETE CASCADE
);

CREATE TABLE student (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL
);

CREATE TABLE book_progress (
    book_id INTEGER,
    student_id INTEGER,
    study_plan TEXT NOT NULL,
    current_progress TEXT NOT NULL,
    update_time DATETIME DEFAULT CURRENT_TIMESTAMP NOT NULL,
    PRIMARY KEY (book_id, student_id),
    FOREIGN KEY (book_id) REFERENCES book(id) ON DELETE CASCADE,
    FOREIGN KEY (student_id) REFERENCES student(id) ON DELETE CASCADE
);

CREATE TABLE history_message (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    student_id INTEGER NOT NULL,
    book_id INTEGER NOT NULL,
    content TEXT NOT NULL,
    update_time DATETIME DEFAULT CURRENT_TIMESTAMP NOT NULL,
    FOREIGN KEY (student_id) REFERENCES student(id) ON DELETE CASCADE,
    FOREIGN KEY (book_id) REFERENCES book(id) ON DELETE CASCADE
);

CREATE TABLE chapter_progress (
    student_id INTEGER NOT NULL,
    book_id INTEGER NOT NULL,
    chapter_number CHAR(20) NOT NULL,
    status INTEGER CHECK(status BETWEEN 0 AND 2) NOT NULL,
    description TEXT NOT NULL,
    update_time DATETIME DEFAULT CURRENT_TIMESTAMP NOT NULL,
    PRIMARY KEY (student_id, book_id, chapter_number),
    FOREIGN KEY (student_id) REFERENCES student(id) ON DELETE CASCADE,
    FOREIGN KEY (book_id) REFERENCES book(id) ON DELETE CASCADE,
    FOREIGN KEY (book_id, chapter_number) REFERENCES chapter(book_id, chapter_number) ON DELETE CASCADE
);
