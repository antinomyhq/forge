use forge_domain::ExecutableTool;
use insta::assert_snapshot;
use tokio::fs;

use crate::outline::{Outline, OutlineInput};
use crate::tools::utils::TempDir;

#[tokio::test]
async fn java_outline() {
    let temp_dir = TempDir::new().unwrap();
    let content = r#"
package com.example.demo;

import java.util.List;

public class UserService {
    private List<User> users;

    public UserService() {
        this.users = new ArrayList<>();
    }

    @Override
    public String toString() {
        return "UserService";
    }

    public void addUser(User user) throws IllegalArgumentException {
        if (user == null) {
            throw new IllegalArgumentException("User cannot be null");
        }
        users.add(user);
    }

    static class User {
        private String name;
        private int age;

        public User(String name, int age) {
            this.name = name;
            this.age = age;
        }
    }

    interface UserValidator {
        boolean validate(User user);
    }
}"#;
    let file_path = temp_dir.path().join("test.java");
    fs::write(&file_path, content).await.unwrap();

    let outline = Outline;
    let result = outline
        .call(OutlineInput { path: temp_dir.path().to_string_lossy().to_string() })
        .await
        .unwrap();

    assert_snapshot!("outline_java", result);
}
