use forge_domain::ExecutableTool;
use insta::assert_snapshot;
use tokio::fs;

use crate::outline::{Outline, OutlineInput};
use crate::tools::utils::TempDir;

#[tokio::test]
async fn css_outline() {
    let temp_dir = TempDir::new().unwrap();
    let content = r#"
@media (max-width: 768px) {
    .container {
        width: 100%;
    }
}

@keyframes fade {
    from { opacity: 0; }
    to { opacity: 1; }
}

.header {
    font-size: 2em;
}

#main-content {
    padding: 20px;
}

@import url('other.css');

:root {
    --primary-color: #333;
}

@supports (display: grid) {
    .grid-layout {
        display: grid;
    }
}"#;
    let file_path = temp_dir.path().join("test.css");
    fs::write(&file_path, content).await.unwrap();

    let outline = Outline;
    let result = outline
        .call(OutlineInput { path: temp_dir.path().to_string_lossy().to_string() })
        .await
        .unwrap();

    assert_snapshot!("outline_css", result);
}
