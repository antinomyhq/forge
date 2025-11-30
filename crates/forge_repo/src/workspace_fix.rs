        let mut connection = self.pool.get_connection()?;
        
        let record = WorkspaceRecord::new(workspace_id, folder_path);
        let wid = workspace_id.id() as i64;
        let path = folder_path.to_string_lossy().to_string();

        // Try to insert new workspace
        let result = diesel::insert_into(workspaces::table)
            .values(&record)
            .on_conflict(workspace_id)
            .do_nothing()
            .execute(&mut connection);