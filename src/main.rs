use serde::{Deserialize, Serialize};
use serde_json;
use std::collections::HashMap;
use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Task {
    id: u32,
    description: String,
    completed: bool,
}

#[derive(Debug, Deserialize)]
struct NewTask {
    description: String,
}

#[derive(Debug, Deserialize)]
struct UpdateTask {
    description: Option<String>,
    completed: Option<bool>,
}

struct TodoApp {
    tasks: Arc<Mutex<HashMap<u32, Task>>>,
    next_id: Arc<Mutex<u32>>,
}

impl TodoApp {
    fn new() -> Self {
        TodoApp {
            tasks: Arc::new(Mutex::new(HashMap::new())),
            next_id: Arc::new(Mutex::new(1)),
        }
    }

    fn create_task(&self, description: String) -> Task {
        let mut next_id = self.next_id.lock().unwrap();
        let id = *next_id;
        *next_id += 1;

        let task = Task {
            id,
            description,
            completed: false,
        };

        self.tasks.lock().unwrap().insert(id, task.clone());
        task
    }

    fn get_all_tasks(&self) -> Vec<Task> {
        let tasks = self.tasks.lock().unwrap();
        tasks.values().cloned().collect()
    }

    fn update_task(
        &self,
        id: u32,
        description: Option<String>,
        completed: Option<bool>,
    ) -> Option<Task> {
        let mut tasks = self.tasks.lock().unwrap();
        println!("{tasks:?}");
        if let Some(task) = tasks.get_mut(&id) {
            if let Some(desc) = description {
                task.description = desc;
            }
            if let Some(status) = completed {
                task.completed = status;
            }
            Some(task.clone())
        } else {
            None
        }
    }

    fn delete_task(&self, id: u32) -> Option<Task> {
        let mut tasks = self.tasks.lock().unwrap();
        tasks.remove(&id)
    }
}

fn handle_request(stream: TcpStream, app: Arc<TodoApp>) {
    let mut buffer = [0; 1024];
    let mut stream = stream;

    // Read the incoming request
    stream.read(&mut buffer).unwrap();

    // Convert the request into a string
    let request = String::from_utf8_lossy(&buffer);

    // Parse the request method and path
    let (method, path) = if let Some(line) = request.lines().next() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        (parts[0], parts[1])
    } else {
        ("", "")
    };

    // Handle different HTTP methods
    match (method, path) {
        ("GET", "/tasks") => {
            let tasks = app.get_all_tasks();
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\r\n{}",
                serde_json::to_string(&tasks).unwrap()
            );
            stream.write(response.as_bytes()).unwrap();
        }
        ("POST", "/tasks") => {
            // Extract the body starting from the first { to the last }
            if let Some(body_start) = request.find('{') {
                if let Some(body_end) = request.rfind('}') {
                    let body = &request[body_start..=body_end]; // Extract the valid JSON part

                    // Remove any trailing null bytes (if they exist)
                    let body = body.trim_end_matches('\0').trim();

                    // Parse the JSON body into a NewTask struct
                    let new_task: Result<NewTask, _> = serde_json::from_str(body);

                    match new_task {
                        Ok(task) => {
                            let created_task = app.create_task(task.description);
                            let response = format!(
                                "HTTP/1.1 201 Created\r\nContent-Type: application/json\r\n\r\n{}",
                                serde_json::to_string(&created_task).unwrap()
                            );
                            stream.write(response.as_bytes()).unwrap();
                        }
                        Err(_) => {
                            let response = "HTTP/1.1 400 Bad Request\r\n\r\nInvalid JSON";
                            stream.write(response.as_bytes()).unwrap();
                        }
                    }
                } else {
                    let response = "HTTP/1.1 400 Bad Request\r\n\r\nInvalid JSON format";
                    stream.write(response.as_bytes()).unwrap();
                }
            } else {
                let response = "HTTP/1.1 400 Bad Request\r\n\r\nInvalid JSON format";
                stream.write(response.as_bytes()).unwrap();
            }
        }
        ("PUT", path) if path.starts_with("/tasks/") => {
            // Extract the task ID from the URL path
            let id_str = &path[7..];
            if let Ok(id) = id_str.parse::<u32>() {
                // Extract the body starting from the first { to the last }
                if let Some(body_start) = request.find('{') {
                    if let Some(body_end) = request.rfind('}') {
                        let body = &request[body_start..=body_end]; // Extract the valid JSON part

                        // Remove any trailing null bytes (if they exist)
                        let body = body.trim_end_matches('\0').trim();

                        // Parse the body as JSON
                        match serde_json::from_str::<UpdateTask>(body) {
                            Ok(body) => {
                                let description = body.description;
                                let completed = body.completed;

                                // Update the task with the parsed values
                                if let Some(task) = app.update_task(id, description, completed) {
                                    let response = format!(
                                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\r\n{}",
                                        serde_json::to_string(&task).unwrap()
                                    );
                                    stream.write(response.as_bytes()).unwrap();
                                } else {
                                    let response = "HTTP/1.1 404 Not Found\r\n\r\nTask not found";
                                    stream.write(response.as_bytes()).unwrap();
                                }
                            }
                            Err(_) => {
                                let response = "HTTP/1.1 400 Bad Request\r\n\r\nInvalid JSON body";
                                stream.write(response.as_bytes()).unwrap();
                            }
                        }
                    } else {
                        let response = "HTTP/1.1 400 Bad Request\r\n\r\nInvalid body format (missing closing bracket)";
                        stream.write(response.as_bytes()).unwrap();
                    }
                } else {
                    let response = "HTTP/1.1 400 Bad Request\r\n\r\nInvalid body format (missing opening bracket)";
                    stream.write(response.as_bytes()).unwrap();
                }
            } else {
                let response = "HTTP/1.1 400 Bad Request\r\n\r\nInvalid task ID";
                stream.write(response.as_bytes()).unwrap();
            }
        }
        ("DELETE", path) if path.starts_with("/tasks/") => {
            let id_str = &path[7..];
            if let Ok(id) = id_str.parse::<u32>() {
                if let Some(task) = app.delete_task(id) {
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\r\n{}",
                        serde_json::to_string(&task).unwrap()
                    );
                    stream.write(response.as_bytes()).unwrap();
                } else {
                    let response = "HTTP/1.1 404 Not Found\r\n\r\nTask not found";
                    stream.write(response.as_bytes()).unwrap();
                }
            } else {
                let response = "HTTP/1.1 400 Bad Request\r\n\r\nInvalid task ID";
                stream.write(response.as_bytes()).unwrap();
            }
        }
        _ => {
            let response = "HTTP/1.1 404 Not Found\r\n\r\nNot Found";
            stream.write(response.as_bytes()).unwrap();
        }
    }
}

fn main() {
    let app = Arc::new(TodoApp::new());

    // Create a TCP listener on localhost:7878
    let listener = TcpListener::bind("127.0.0.1:7878").unwrap();
    println!("Listening on 127.0.0.1:7878");

    // Accept incoming TCP connections
    for stream in listener.incoming() {
        let stream = stream.unwrap();

        let app = Arc::clone(&app);
        thread::spawn(move || {
            handle_request(stream, app);
        });
    }
}
