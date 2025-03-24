use forge_domain::ExecutableTool;
use insta::assert_snapshot;
use tokio::fs;

use crate::outline::{Outline, OutlineInput};
use crate::tools::utils::TempDir;

#[tokio::test]
async fn tsx_outline() {
    let temp_dir = TempDir::new().unwrap();
    let content = r#"
interface Props {
    name: string;
    age: number;
}

function UserProfile({ name, age }: Props) {
    return (
        <div>
            <h1>{name}</h1>
            <p>Age: {age}</p>
        </div>
    );
}

const UserList: React.FC<{ users: Props[] }> = ({ users }) => {
    return (
        <ul>
            {users.map(user => (
                <UserProfile key={user.name} {...user} />
            ))}
        </ul>
    );
};

export class UserContainer extends React.Component<Props, { loading: boolean }> {
    state = { loading: true };

    componentDidMount() {
        this.setState({ loading: false });
    }

    render() {
        return this.state.loading ? <div>Loading...</div> : <UserProfile {...this.props} />;
    }
}"#;
    let file_path = temp_dir.path().join("test.tsx");
    fs::write(&file_path, content).await.unwrap();

    let outline = Outline;
    let result = outline
        .call(OutlineInput { path: temp_dir.path().to_string_lossy().to_string() })
        .await
        .unwrap();

    assert_snapshot!("outline_tsx", result);
}
