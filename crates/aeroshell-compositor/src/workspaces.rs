//! Estado puro de workspaces; o compositor publica eventos na borda IPC.

use aeroshell_ipc::types::Workspace;

#[derive(Debug)]
pub struct WorkspaceManager {
    workspaces: Vec<Workspace>,
    active_id: u32,
    next_id: u32,
}

impl Default for WorkspaceManager {
    fn default() -> Self {
        Self {
            workspaces: vec![Workspace {
                id: 1,
                name: "Workspace 1".to_owned(),
                window_ids: Vec::new(),
            }],
            active_id: 1,
            next_id: 2,
        }
    }
}

impl WorkspaceManager {
    pub fn list(&self) -> &[Workspace] {
        &self.workspaces
    }

    pub fn active_id(&self) -> u32 {
        self.active_id
    }

    pub fn create(&mut self, name: Option<String>) -> Workspace {
        let id = self.next_id;
        self.next_id = self.next_id.saturating_add(1);
        let name = name
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| format!("Workspace {id}"));
        let workspace = Workspace {
            id,
            name,
            window_ids: Vec::new(),
        };
        self.workspaces.push(workspace.clone());
        workspace
    }

    pub fn rename(&mut self, id: u32, name: String) -> Result<(), String> {
        let name = name.trim();
        if name.is_empty() {
            return Err("ERR nome vazio".to_owned());
        }
        let workspace = self
            .workspaces
            .iter_mut()
            .find(|workspace| workspace.id == id)
            .ok_or_else(|| "ERR workspace inexistente".to_owned())?;
        workspace.name = name.to_owned();
        Ok(())
    }

    pub fn activate(&mut self, id: u32) -> Result<(), String> {
        if !self.workspaces.iter().any(|workspace| workspace.id == id) {
            return Err("ERR workspace inexistente".to_owned());
        }
        self.active_id = id;
        Ok(())
    }

    /// Toplevel novo entra no workspace ativo. Também remove uma associação
    /// anterior, mantendo a operação idempotente para o map do xdg-shell.
    pub fn assign_window(&mut self, window_id: u32) -> u32 {
        self.remove_window(window_id);
        if let Some(workspace) = self
            .workspaces
            .iter_mut()
            .find(|workspace| workspace.id == self.active_id)
        {
            workspace.window_ids.push(window_id);
            workspace.window_ids.sort_unstable();
        }
        self.active_id
    }

    pub fn move_window(&mut self, window_id: u32, workspace_id: u32) -> Result<(), String> {
        if !self
            .workspaces
            .iter()
            .any(|workspace| workspace.id == workspace_id)
        {
            return Err("ERR workspace inexistente".to_owned());
        }
        self.remove_window(window_id);
        if let Some(workspace) = self
            .workspaces
            .iter_mut()
            .find(|workspace| workspace.id == workspace_id)
        {
            workspace.window_ids.push(window_id);
            workspace.window_ids.sort_unstable();
        }
        Ok(())
    }

    pub fn remove_window(&mut self, window_id: u32) {
        for workspace in &mut self.workspaces {
            workspace.window_ids.retain(|id| *id != window_id);
        }
    }

    /// Devolve o workspace removido e, se ele era ativo, o novo ID ativado.
    pub fn destroy(&mut self, id: u32) -> Result<(Workspace, Option<u32>), String> {
        if self.workspaces.len() == 1 {
            return Err("ERR último workspace".to_owned());
        }
        let index = self
            .workspaces
            .iter()
            .position(|workspace| workspace.id == id)
            .ok_or_else(|| "ERR workspace inexistente".to_owned())?;
        if !self.workspaces[index].window_ids.is_empty() {
            return Err("ERR workspace não vazio".to_owned());
        }
        let removed = self.workspaces.remove(index);
        let activated = if self.active_id == id {
            self.active_id = self.workspaces[0].id;
            Some(self.active_id)
        } else {
            None
        };
        Ok((removed, activated))
    }

    pub fn window_visible(&self, window_id: u32) -> bool {
        self.workspaces.iter().any(|workspace| {
            workspace.id == self.active_id && workspace.window_ids.contains(&window_id)
        })
    }

    pub fn workspace_of_window(&self, window_id: u32) -> Option<u32> {
        self.workspaces
            .iter()
            .find(|workspace| workspace.window_ids.contains(&window_id))
            .map(|workspace| workspace.id)
    }
}

#[cfg(test)]
mod tests {
    use super::WorkspaceManager;

    #[test]
    fn cria_renomeia_ativa_e_move_em_ordem_estavel() {
        let mut manager = WorkspaceManager::default();
        assert_eq!(manager.active_id(), 1);
        assert_eq!(manager.assign_window(7), 1);

        let second = manager.create(Some("Projeto Aero".to_owned()));
        assert_eq!(second.id, 2);
        assert_eq!(second.name, "Projeto Aero");
        manager.rename(2, "Aero Shell".to_owned()).expect("rename");
        manager.activate(2).expect("activate");
        manager.move_window(7, 2).expect("move");
        manager.assign_window(3);

        assert!(manager.list()[0].window_ids.is_empty());
        assert_eq!(manager.list()[1].window_ids, [3, 7]);
        assert!(manager.window_visible(3));
        assert!(manager.window_visible(7));
        manager.activate(1).expect("reactivate");
        assert!(!manager.window_visible(3));
        assert!(!manager.window_visible(7));
    }

    #[test]
    fn destroy_recusa_ultimo_ou_nao_vazio_e_esvazia_no_unmap() {
        let mut manager = WorkspaceManager::default();
        assert!(manager.destroy(1).is_err());
        let second = manager.create(None);
        manager.assign_window(9);
        manager.move_window(9, second.id).expect("move");
        assert!(manager.destroy(second.id).is_err());
        manager.remove_window(9);
        assert!(manager
            .list()
            .iter()
            .all(|workspace| workspace.window_ids.is_empty()));
        let (_, activated) = manager.destroy(second.id).expect("destroy empty");
        assert_eq!(activated, None);
        assert_eq!(manager.active_id(), 1);
    }

    #[test]
    fn destruir_ativa_escolhe_outra_e_visibilidade_desconhecida_e_falsa() {
        let mut manager = WorkspaceManager::default();
        let second = manager.create(None);
        manager.activate(second.id).expect("activate");
        let (_, activated) = manager.destroy(second.id).expect("destroy active");
        assert_eq!(activated, Some(1));
        assert_eq!(manager.active_id(), 1);
        assert!(!manager.window_visible(999));
        assert!(manager.activate(999).is_err());
        assert!(manager.move_window(1, 999).is_err());
    }
}
