use forge_domain::ExecutableTool;
use insta::assert_snapshot;
use tokio::fs;

use super::super::{Outline, OutlineInput};
use crate::tools::utils::TempDir;

#[tokio::test]
async fn typescript_outline() {
    let temp_dir = TempDir::new().unwrap();
    let content = r#"
interface User {
    name: string;
    age: number;
}

type UserResponse = {
    user: User;
    status: 'active' | 'inactive';
};

class UserService {
    private users: User[] = [];

    constructor() {}

    async addUser(user: User): Promise<void> {
        this.users.push(user);
    }

    static getInstance(): UserService {
        return new UserService();
    }
}

enum UserRole {
    Admin = 'ADMIN',
    User = 'USER'
}

async function fetchUser(id: string): Promise<User> {
    return {} as User;
}

const processUser = (user: User): UserResponse => {
    return {
        user,
        status: 'active'
    };
};"#;
    let file_path = temp_dir.path().join("test.ts");
    fs::write(&file_path, content).await.unwrap();

    let outline = Outline;
    let result = outline
        .call(OutlineInput { path: temp_dir.path().to_string_lossy().to_string() })
        .await
        .unwrap();

    assert_snapshot!("outline_typescript", result);
}
