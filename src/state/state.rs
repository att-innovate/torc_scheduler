// The MIT License (MIT)
//
// Copyright (c) 2016 AT&T
//
// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to deal
// in the Software without restriction, including without limitation the rights
// to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
// copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in
// all copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
// THE SOFTWARE.

use chrono::UTC;
use collaborator::{add_route, delete_route, kill_task, register_running_task, reset_fib};
use std::fs::File;
use std::io::Read;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread;
use std::time::Duration;
use super::node_list::{Node, NodeList};
use super::task_list::{SLA, Task, TaskList, Volume};
use utils::{read_int, read_string, read_string_replace_variable};
use uuid::Uuid;
use yaml_rust::{Yaml, YamlLoader};

#[derive (Clone)]
pub struct StateManager {
    sender: Sender<StateRequestMsg>,
    master_ip: String,
    my_name: String,
    my_ip: String,
    my_framework_id: String,
    ipmi_proxy: String,
    network_agent_type: String,
    network_agent_connection: String,
    config: Yaml,
}

#[derive(Clone, Hash, Eq, PartialEq, Debug, RustcEncodable, RustcDecodable)]
pub enum TaskState {
    NotRunning,
    Restart,
    Requested,
    Accepted,
    Running,
}


impl StateManager {
    pub fn new(master_ip: String, my_ip: String, config_file: String) -> StateManager {
        let (tx, rx) = channel();
        let config = StateManager::read_config_file(config_file);
        let my_name = config["name"].as_str().unwrap_or("torc-controller").to_string();
        let ipmi_proxy = config["ipmiproxy"].as_str().unwrap_or("undefined").to_string();
        let network_agent_type = config["network-agent"]["type"].as_str().unwrap_or("undefined").to_string();
        let mut network_agent_connection = config["network-agent"]["connection"].as_str().unwrap_or("undefined").to_string();
        network_agent_connection = str::replace(&network_agent_connection, "$MASTER_IP", &master_ip);

        let statemanager = StateManager {
            sender: tx,
            master_ip: master_ip.clone(),
            my_name: my_name.clone(),
            my_ip: my_ip,
            my_framework_id: format!("{}-{}", my_name.clone(), Uuid::new_v4().to_simple_string()),
            ipmi_proxy: ipmi_proxy.clone(),
            network_agent_type: network_agent_type.clone(),
            network_agent_connection: network_agent_connection.clone(),
            config: config,
        };

        statemanager.start_serving(rx);
        statemanager.load_node_list();
        statemanager.start_syncing();
        statemanager.start_cleaning();

        reset_fib(&network_agent_type, &network_agent_connection);

        statemanager
    }

    pub fn get_master_ip(&self) -> String {
        self.master_ip.clone()
    }

    pub fn get_my_name(&self) -> String {
        self.my_name.clone()
    }

    pub fn get_ipmi_proxy(&self) -> String {
        self.ipmi_proxy.clone()
    }

    pub fn get_my_framework_id(&self) -> String {
        self.my_framework_id.clone()
    }

    pub fn get_my_ip(&self) -> String {
        self.my_ip.clone()
    }

    pub fn get_network_agent_type(&self) -> String {
        self.network_agent_type.clone()
    }

    pub fn get_network_agent_connection(&self) -> String {
        self.network_agent_connection.clone()
    }

    pub fn get_yaml(&self) -> Yaml {
        self.config.clone()
    }

    pub fn send_ping(&self) {
        let (sender, receiver) = channel();

        let msg = StateRequestMsg::Ping { sender: sender };
        self.sender.send(msg).unwrap();
        receiver.recv().unwrap();
    }

    pub fn request_task_state(&self, task_name: String) -> TaskState {
        let (sender, receiver) = channel();

        let msg = StateRequestMsg::GetTaskState {
            sender: sender,
            task_name: task_name,
        };
        self.sender.send(msg).unwrap();

        let state = match receiver.recv().unwrap() {
            StateResponseMsg::TaskState { task_state } => task_state,
            _ => TaskState::NotRunning,
        };

        state
    }

    pub fn request_task_ip(&self, task_name: String) -> String {
        let (sender, receiver) = channel();

        let msg = StateRequestMsg::GetTaskIP {
            sender: sender,
            task_name: task_name,
        };
        self.sender.send(msg).unwrap();

        let ip = match receiver.recv().unwrap() {
            StateResponseMsg::TaskIP { task_ip } => task_ip,
            _ => "".to_string(),
        };

        ip
    }

    pub fn request_task_name_by_id(&self, id_prefix: String) -> String {
        let (sender, receiver) = channel();

        let msg = StateRequestMsg::GetTaskNameById {
            sender: sender,
            id_prefix: id_prefix,
        };
        self.sender.send(msg).unwrap();

        let task_name: String = match receiver.recv().unwrap() {
            StateResponseMsg::TaskName { task_name } => task_name,
            _ => "".to_string(),
        };

        task_name.clone()
    }

    pub fn send_update_task_state(&self, task_name: String, task_state: TaskState) {
        let (sender, receiver) = channel();

        let msg = StateRequestMsg::UpdateTaskState {
            sender: sender,
            task_name: task_name,
            task_state: task_state,
        };
        self.sender.send(msg).unwrap();
        receiver.recv().unwrap();
    }

    pub fn send_update_task_node_name(&self, task_name: String, node_name: String) {
        let (sender, receiver) = channel();

        let msg = StateRequestMsg::UpdateTaskNodeName {
            sender: sender,
            task_name: task_name,
            node_name: node_name,
        };
        self.sender.send(msg).unwrap();
        receiver.recv().unwrap();
    }

    pub fn send_update_task_info(&self, task_name: String, id: String, ip: String, slave_id: String) {
        let (sender, receiver) = channel();

        let msg = StateRequestMsg::UpdateTaskInfo {
            sender: sender,
            task_name: task_name,
            id: id,
            ip: ip,
            slave_id: slave_id,
        };
        self.sender.send(msg).unwrap();
        receiver.recv().unwrap();
    }

    pub fn send_start_task(&self,
                           name: &String,
                           image: &String,
                           node_name: &String,
                           node_type: &String,
                           node_function: &String,
                           dependent_service: &String,
                           arguments: &String,
                           parameters: &String,
                           memory: &f64,
                           cpu: &f64,
                           volumes: &Vec<Volume>,
                           privileged: &bool,
                           sla: &SLA,
                           is_metered: &bool,
                           is_system_service: &bool,
                           is_job: &bool,
                           network_type: &String) {

        let (sender, receiver) = channel();

        let new_task = Task {
            name: name.clone(),
            controller: self.get_my_name(),
            id: "".to_string(),
            image: image.clone(),
            node_name: node_name.clone(),
            node_type: node_type.clone(),
            node_function: node_function.clone(),
            dependent_service: dependent_service.clone(),
            arguments: arguments.clone(),
            parameters: parameters.clone(),
            memory: memory.clone(),
            cpu: cpu.clone(),
            privileged: privileged.clone(),
            sla: sla.clone(),
            is_metered: is_metered.clone(),
            is_system_service: is_system_service.clone(),
            is_job: is_job.clone(),
            volumes: volumes.clone(),
            network_type: network_type.clone(),
            ip: "".to_string(),
            slave_id: "".to_string(),
            state: TaskState::Requested,
            last_update: UTC::now().timestamp(),
        };

        let msg = StateRequestMsg::StartTask {
            sender: sender,
            task: new_task,
        };

        self.sender.send(msg).unwrap();
        receiver.recv().unwrap();
    }

    pub fn send_restart_task(&self, task_name: String) {
        let (sender, receiver) = channel();

        let msg = StateRequestMsg::RestartTask {
            sender: sender,
            task_name: task_name,
        };
        self.sender.send(msg).unwrap();
        receiver.recv().unwrap();
    }

    pub fn request_is_restartable_task(&self, task_name: String) -> bool {
        let (sender, receiver) = channel();

        let msg = StateRequestMsg::GetIsRestartableTask {
            sender: sender,
            task_name: task_name,
        };
        self.sender.send(msg).unwrap();

        let is_system_task = match receiver.recv().unwrap() {
            StateResponseMsg::GetIsRestartableTask { is_restartable_task } => is_restartable_task,
            _ => false,
        };

        is_system_task
    }

    pub fn send_kill_task_by_name(&self, task_name: String) {
        kill_task(&task_name);
    }

    pub fn send_remove_task_by_name(&self, task_name: String) {
        let (sender, receiver) = channel();

        let msg = StateRequestMsg::RemoveTask {
            sender: sender,
            task_name: task_name,
        };
        self.sender.send(msg).unwrap();
        receiver.recv().unwrap();
    }

    pub fn send_announce_task(&self, task: &Task) {
        let (sender, receiver) = channel();

        if self.request_task_name_by_id(task.id.clone()).len() > 0 {
            let msg = StateRequestMsg::UpdateTaskLastUpdate {
                sender: sender,
                task_name: task.name.clone(),
            };

            self.sender.send(msg).unwrap();
        } else {
            match self.request_node(task.node_name.clone()) {
                Some(node) => {
                    add_route(&self.get_network_agent_type(),
                              &self.get_network_agent_connection(),
                              &task.ip,
                              &node.external_ip)
                }
                _ => {}
            }

            // just in case it hasn't get cleaned up yet.
            let ip = self.request_task_ip(task.name.clone());
            if ip.len() > 0 {
                delete_route(&self.get_network_agent_type(),
                             &self.get_network_agent_connection(),
                             &ip);
            }

            let msg = StateRequestMsg::StartTask {
                sender: sender,
                task: task.clone(),
            };

            self.sender.send(msg).unwrap();
        }
        receiver.recv().unwrap();
    }

    pub fn request_list_requested_tasks(&self) -> Vec<Task> {
        let (sender, receiver) = channel();

        let msg = StateRequestMsg::GetRequestedTasks { sender: sender };
        self.sender.send(msg).unwrap();

        let result: Vec<Task> = match receiver.recv().unwrap() {
            StateResponseMsg::GetRequestedTasks { requested_tasks } => requested_tasks,
            _ => vec![],
        };

        result
    }

    pub fn request_list_running_tasks(&self) -> Vec<Task> {
        let (sender, receiver) = channel();

        let msg = StateRequestMsg::GetRunningTasks { sender: sender };
        self.sender.send(msg).unwrap();

        let result: Vec<Task> = match receiver.recv().unwrap() {
            StateResponseMsg::GetRunningTasks { running_tasks } => running_tasks,
            _ => vec![],
        };

        result
    }

    pub fn request_list_restart_tasks(&self) -> Vec<Task> {
        let (sender, receiver) = channel();

        let msg = StateRequestMsg::GetRestartTasks { sender: sender };
        self.sender.send(msg).unwrap();

        let result: Vec<Task> = match receiver.recv().unwrap() {
            StateResponseMsg::GetRestartTasks { restart_tasks } => restart_tasks,
            _ => vec![],
        };

        result
    }

    pub fn send_add_node(&self,
                         name: String,
                         ip: String,
                         external_ip: String,
                         management_ip: String,
                         port_id: i64,
                         node_type: String) {
        let (sender, receiver) = channel();

        let new_node = Node {
            name: name.clone(),
            ip: ip.clone(),
            external_ip: external_ip.clone(),
            management_ip: management_ip.clone(),
            node_type: node_type.clone(),
            node_function: "none".to_string(),
            active: false,
            slave_id: "".to_string(),
            port_id: port_id,
            last_seen: UTC::now().timestamp(),
        };

        let msg = StateRequestMsg::AddNode {
            sender: sender,
            node: new_node,
        };

        self.sender.send(msg).unwrap();
        receiver.recv().unwrap();
    }

    pub fn request_is_node_active(&self, node_name: String) -> bool {
        let (sender, receiver) = channel();

        let msg = StateRequestMsg::GetIsNodeActive {
            sender: sender,
            node_name: node_name,
        };
        self.sender.send(msg).unwrap();

        let is_active = match receiver.recv().unwrap() {
            StateResponseMsg::GetIsNodeActive { is_active } => is_active,
            _ => false,
        };

        is_active
    }
    pub fn send_set_node_inactive(&self, node_name: String) {
        let (sender, receiver) = channel();

        let msg = StateRequestMsg::SetNodeInactive {
            sender: sender,
            node_name: node_name,
        };

        self.sender.send(msg).unwrap();
        receiver.recv().unwrap();

    }

    pub fn send_update_node(&self, node_name: String, node_type: String, node_function: String, slave_id: String) {
        let (sender, receiver) = channel();

        let msg = StateRequestMsg::UpdateNode {
            sender: sender,
            node_name: node_name,
            node_type: node_type,
            node_function: node_function,
            slave_id: slave_id,
        };
        self.sender.send(msg).unwrap();
        receiver.recv().unwrap();
    }

    pub fn request_node(&self, node_name: String) -> Option<Node> {
        let (sender, receiver) = channel();

        let msg = StateRequestMsg::GetNode {
            sender: sender,
            node_name: node_name,
        };
        self.sender.send(msg).unwrap();

        let result = match receiver.recv().unwrap() {
            StateResponseMsg::GetNode { node } => Some(node),
            _ => None,
        };

        result
    }

    pub fn request_list_nodes(&self) -> Vec<Node> {
        let (sender, receiver) = channel();

        let msg = StateRequestMsg::GetNodes { sender: sender };
        self.sender.send(msg).unwrap();

        let result: Vec<Node> = match receiver.recv().unwrap() {
            StateResponseMsg::GetNodes { nodes } => nodes,
            _ => vec![],
        };

        result
    }
}

struct State {
    initialized: bool,
    master_ip: String,
    my_name: String,
    task_list: TaskList,
    node_list: NodeList,
}

enum StateRequestMsg {
    Ping { sender: Sender<StateResponseMsg> },
    GetTaskState {
        sender: Sender<StateResponseMsg>,
        task_name: String,
    },
    GetTaskIP {
        sender: Sender<StateResponseMsg>,
        task_name: String,
    },
    GetTaskNameById {
        sender: Sender<StateResponseMsg>,
        id_prefix: String,
    },
    UpdateTaskState {
        sender: Sender<StateResponseMsg>,
        task_name: String,
        task_state: TaskState,
    },
    UpdateTaskNodeName {
        sender: Sender<StateResponseMsg>,
        task_name: String,
        node_name: String,
    },
    UpdateTaskInfo {
        sender: Sender<StateResponseMsg>,
        task_name: String,
        id: String,
        ip: String,
        slave_id: String,
    },
    UpdateTaskLastUpdate {
        sender: Sender<StateResponseMsg>,
        task_name: String,
    },
    StartTask {
        sender: Sender<StateResponseMsg>,
        task: Task,
    },
    RestartTask {
        sender: Sender<StateResponseMsg>,
        task_name: String,
    },
    RemoveTask {
        sender: Sender<StateResponseMsg>,
        task_name: String,
    },
    GetIsRestartableTask {
        sender: Sender<StateResponseMsg>,
        task_name: String,
    },
    GetRequestedTasks { sender: Sender<StateResponseMsg> },
    GetRunningTasks { sender: Sender<StateResponseMsg> },
    GetRestartTasks { sender: Sender<StateResponseMsg> },
    AddNode {
        sender: Sender<StateResponseMsg>,
        node: Node,
    },
    GetIsNodeActive {
        sender: Sender<StateResponseMsg>,
        node_name: String,
    },
    UpdateNode {
        sender: Sender<StateResponseMsg>,
        node_name: String,
        node_type: String,
        node_function: String,
        slave_id: String,
    },
    SetNodeInactive {
        sender: Sender<StateResponseMsg>,
        node_name: String,
    },
    GetNode {
        sender: Sender<StateResponseMsg>,
        node_name: String,
    },
    GetNodes { sender: Sender<StateResponseMsg> },
}

enum StateResponseMsg {
    Pong,
    TaskState { task_state: TaskState },
    TaskIP { task_ip: String },
    TaskName { task_name: String },
    UpdateTaskState,
    UpdateTaskInfo,
    UpdateTaskNodeName,
    UpdateTaskLastUpdate,
    StartTask,
    RestartTask,
    RemoveTask,
    GetIsRestartableTask { is_restartable_task: bool },
    GetRequestedTasks { requested_tasks: Vec<Task> },
    GetRunningTasks { running_tasks: Vec<Task> },
    GetRestartTasks { restart_tasks: Vec<Task> },
    AddNode,
    GetIsNodeActive { is_active: bool },
    UpdateNode,
    SetNodeInactive,
    GetNodes { nodes: Vec<Node> },
    GetNode { node: Node },
}


impl StateManager {
    fn read_config_file(config_file: String) -> Yaml {
        let mut file = match File::open(config_file) {
            Ok(file) => file,
            Err(err) => panic!(err.to_string()),
        };

        let mut content = String::new();
        file.read_to_string(&mut content).unwrap();
        let config = YamlLoader::load_from_str(&content).unwrap();
        // Multi document support, doc is a yaml::Yaml
        config[0].clone()
    }

    fn start_serving(&self, rx: Receiver<StateRequestMsg>) {
        let master_ip = self.get_master_ip();
        let my_name = self.get_my_name();
        thread::Builder::new()
            .name("state-serve".to_string())
            .spawn(move || {
                let mut state = State {
                    initialized: false,
                    master_ip: master_ip,
                    my_name: my_name,
                    task_list: TaskList::new(),
                    node_list: NodeList::new(),
                };
                state.initialized = true;

                loop {
                    match rx.recv().unwrap() {
                        StateRequestMsg::Ping { sender } => StateManager::ping(sender),
                        StateRequestMsg::GetTaskState { sender, task_name } => {
                            StateManager::get_task_state(sender, &state, task_name)
                        }
                        StateRequestMsg::GetTaskIP { sender, task_name } => StateManager::get_task_ip(sender, &state, task_name),
                        StateRequestMsg::GetTaskNameById { sender, id_prefix } => {
                            StateManager::get_task_name_by_id(sender, &state, id_prefix)
                        }
                        StateRequestMsg::UpdateTaskState { sender, task_name, task_state } => {
                            StateManager::update_task_state(sender, &state, task_name, task_state)
                        }
                        StateRequestMsg::UpdateTaskNodeName { sender, task_name, node_name } => {
                            StateManager::update_task_node_name(sender, &state, task_name, node_name)
                        }
                        StateRequestMsg::UpdateTaskInfo { sender, task_name, id, ip, slave_id } => {
                            StateManager::update_task_info(sender, &state, task_name, id, ip, slave_id)
                        }
                        StateRequestMsg::UpdateTaskLastUpdate { sender, task_name } => {
                            StateManager::update_task_last_update(sender, &state, task_name)
                        }
                        StateRequestMsg::StartTask { sender, task } => StateManager::start_task(sender, &state, &task),
                        StateRequestMsg::RestartTask { sender, task_name } => StateManager::restart_task(sender, &state, task_name),
                        StateRequestMsg::RemoveTask { sender, task_name } => {
                            StateManager::remove_task_by_name(sender, &state, task_name)
                        }
                        StateRequestMsg::GetIsRestartableTask { sender, task_name } => {
                            StateManager::get_is_restartable_task(sender, &state, task_name)
                        }
                        StateRequestMsg::GetRequestedTasks { sender } => StateManager::get_requested_tasks(sender, &state),
                        StateRequestMsg::GetRunningTasks { sender } => StateManager::get_running_tasks(sender, &state),
                        StateRequestMsg::GetRestartTasks { sender } => StateManager::get_restart_tasks(sender, &state),
                        StateRequestMsg::AddNode { sender, node } => StateManager::add_node(sender, &state, &node),
                        StateRequestMsg::GetIsNodeActive { sender, node_name } => {
                            StateManager::get_is_node_active(sender, &state, node_name)
                        }
                        StateRequestMsg::UpdateNode { sender, node_name, node_type, node_function, slave_id } => {
                            StateManager::update_node(sender,
                                                      &state,
                                                      node_name,
                                                      node_type,
                                                      node_function,
                                                      slave_id)
                        }
                        StateRequestMsg::SetNodeInactive { sender, node_name } => {
                            StateManager::set_node_inactive(sender, &state, node_name)
                        }
                        StateRequestMsg::GetNode { sender, node_name } => StateManager::get_node(sender, &state, node_name),
                        StateRequestMsg::GetNodes { sender } => StateManager::get_nodes(sender, &state),
                    }
                }
            })
            .unwrap();
    }

    fn start_syncing(&self) {
        let config = self.get_yaml();
        let wait_time = config["statesync"]["poll_interval_in_seconds"].as_i64().unwrap() as u64;
        let state_manager = self.clone();
        let master_ip = self.master_ip.clone();
        let my_name = self.get_my_name();

        thread::Builder::new()
            .name("state-sync".to_string())
            .spawn(move || {
                loop {
                    thread::sleep(Duration::from_secs(wait_time));
                    println!("syncing ....");
                    let running_tasks = state_manager.request_list_running_tasks();
                    for task in &running_tasks {
                        register_running_task(&master_ip, &task);
                        if task.controller == my_name {
                            state_manager.send_announce_task(&task);
                        }
                    }
                }
            })
            .unwrap();
    }

    fn start_cleaning(&self) {
        let config = self.get_yaml();
        let wait_time = config["stateclean"]["poll_interval_in_seconds"].as_i64().unwrap() as u64;
        let timeout = config["stateclean"]["timeout_in_seconds"].as_i64().unwrap() as i64;
        let restart_delay = config["stateclean"]["restart_delay_in_seconds"].as_i64().unwrap() as i64;
        let state_manager = self.clone();
        let my_name = self.get_my_name();

        thread::Builder::new()
            .name("state-clean".to_string())
            .spawn(move || {
                loop {
                    thread::sleep(Duration::from_secs(wait_time));
                    println!("cleaning ...");
                    let running_tasks = state_manager.request_list_running_tasks();
                    for task in &running_tasks {
                        if task.controller == my_name {
                            continue;
                        };
                        let now = UTC::now().timestamp();
                        if (task.last_update + timeout) < now {
                            state_manager.send_remove_task_by_name(task.name.clone());
                            delete_route(&state_manager.get_network_agent_type(),
                                         &state_manager.get_network_agent_connection(),
                                         &task.ip);
                        }
                    }

                    let restart_tasks = state_manager.request_list_restart_tasks();
                    for task in &restart_tasks {
                        if task.controller != my_name {
                            continue;
                        };
                        let now = UTC::now().timestamp();
                        if (task.last_update + restart_delay) < now {
                            state_manager.send_update_task_state(task.name.clone(), TaskState::Requested);
                        }
                    }

                    let nodes = state_manager.request_list_nodes();
                    for node in &nodes {
                        if node.active == false {
                            continue;
                        }
                        let now = UTC::now().timestamp();
                        if (node.last_seen + timeout) < now {
                            state_manager.send_set_node_inactive(node.name.clone());
                        }

                    }
                }
            })
            .unwrap();
    }

    fn load_node_list(&self) {
        let config = self.get_yaml();
        let nodes = config["nodes"].as_vec().unwrap();
        for node in nodes {
            self.send_add_node(read_string(node, "name".to_string()),
                               read_string_replace_variable(node, "ip".to_string(), &self),
                               read_string_replace_variable(node, "external_ip".to_string(), &self),
                               read_string(node, "management_ip".to_string()),
                               read_int(node, "port".to_string(), 0),
                               read_string(node, "type".to_string()))
        }
    }

    fn ping(sender: Sender<StateResponseMsg>) {
        println!("got ping");
        let msg = StateResponseMsg::Pong;
        sender.send(msg).unwrap();
    }

    fn get_task_state(sender: Sender<StateResponseMsg>, state: &State, task_name: String) {
        let task_state = state.task_list.get_task_state(task_name);
        let msg = StateResponseMsg::TaskState { task_state: task_state };
        sender.send(msg).unwrap();
    }

    fn get_task_ip(sender: Sender<StateResponseMsg>, state: &State, task_name: String) {
        let result = state.task_list.get_task(task_name.clone());
        let ip = match result {
            Ok(task) => task.ip.clone(),
            Err(_) => "".to_string(),
        };
        let msg = StateResponseMsg::TaskIP { task_ip: ip };
        sender.send(msg).unwrap();
    }

    fn get_task_name_by_id(sender: Sender<StateResponseMsg>, state: &State, id_prefix: String) {
        let task_name = state.task_list.get_task_name_by_id(id_prefix);
        let msg = StateResponseMsg::TaskName { task_name: task_name };
        sender.send(msg).unwrap();
    }

    fn update_task_state(sender: Sender<StateResponseMsg>, state: &State, task_name: String, task_state: TaskState) {
        state.task_list.set_task_state(task_name.to_string(), task_state.clone());

        match task_state {
            TaskState::Running => {
                let result = state.task_list.get_task(task_name.clone());
                match result {
                    Ok(task) => register_running_task(&state.master_ip.clone(), &task),
                    Err(error_msg) => {
                        println!("error [{:?}] while retrieving {}",
                                 error_msg,
                                 task_name.clone())
                    }
                }

            }
            _ => {}
        }

        let msg = StateResponseMsg::UpdateTaskState;
        sender.send(msg).unwrap();
    }

    fn update_task_node_name(sender: Sender<StateResponseMsg>, state: &State, task_name: String, node_name: String) {
        state.task_list.set_task_node_name(task_name.to_string(), node_name);

        let msg = StateResponseMsg::UpdateTaskNodeName;
        sender.send(msg).unwrap();
    }

    fn update_task_info(sender: Sender<StateResponseMsg>,
                        state: &State,
                        task_name: String,
                        id: String,
                        ip: String,
                        slave_id: String) {
        state.task_list.set_task_info(task_name.to_string(), id, ip, slave_id);

        let msg = StateResponseMsg::UpdateTaskInfo;
        sender.send(msg).unwrap();
    }

    fn update_task_last_update(sender: Sender<StateResponseMsg>, state: &State, task_name: String) {
        state.task_list.update_task_last_update(task_name.to_string());

        let msg = StateResponseMsg::UpdateTaskLastUpdate;
        sender.send(msg).unwrap();
    }

    fn start_task(sender: Sender<StateResponseMsg>, state: &State, task: &Task) {
        println!("start task {}", task.name);

        state.task_list.add_new_task(&task);
        let msg = StateResponseMsg::StartTask;
        sender.send(msg).unwrap();
    }

    fn restart_task(sender: Sender<StateResponseMsg>, state: &State, task_name: String) {
        println!("restart task {}", task_name);
        state.task_list.update_task_last_update(task_name.clone());
        state.task_list.set_task_state(task_name.clone(), TaskState::Restart);
        let msg = StateResponseMsg::RestartTask;
        sender.send(msg).unwrap();
    }

    fn get_is_restartable_task(sender: Sender<StateResponseMsg>, state: &State, task_name: String) {
        let result = state.task_list.get_task(task_name.clone());
        let is_restartable_task = match result {
            Ok(task) => task.is_system_service && task.controller == state.my_name && task.is_job == false,
            Err(_) => false,
        };

        let msg = StateResponseMsg::GetIsRestartableTask { is_restartable_task: is_restartable_task };
        sender.send(msg).unwrap();
    }

    fn remove_task_by_name(sender: Sender<StateResponseMsg>, state: &State, task_name: String) {
        println!("remove task {}", task_name);

        state.task_list.remove_task_by_name(task_name.to_string());
        let msg = StateResponseMsg::RemoveTask;
        sender.send(msg).unwrap();
    }

    fn get_requested_tasks(sender: Sender<StateResponseMsg>, state: &State) {
        let result: Vec<Task> = state.task_list.get_tasks_with_state(TaskState::Requested);
        let msg = StateResponseMsg::GetRequestedTasks { requested_tasks: result };
        sender.send(msg).unwrap();
    }

    fn get_running_tasks(sender: Sender<StateResponseMsg>, state: &State) {
        let result: Vec<Task> = state.task_list.get_tasks_with_state(TaskState::Running);
        let msg = StateResponseMsg::GetRunningTasks { running_tasks: result };
        sender.send(msg).unwrap();
    }

    fn get_restart_tasks(sender: Sender<StateResponseMsg>, state: &State) {
        let result: Vec<Task> = state.task_list.get_tasks_with_state(TaskState::Restart);
        let msg = StateResponseMsg::GetRestartTasks { restart_tasks: result };
        sender.send(msg).unwrap();
    }

    fn add_node(sender: Sender<StateResponseMsg>, state: &State, node: &Node) {
        state.node_list.add_new_node(&node);
        let msg = StateResponseMsg::AddNode;
        sender.send(msg).unwrap();
    }

    fn get_is_node_active(sender: Sender<StateResponseMsg>, state: &State, node_name: String) {
        let is_active = state.node_list.is_node_active(node_name.clone());
        let msg = StateResponseMsg::GetIsNodeActive { is_active: is_active };
        sender.send(msg).unwrap();
    }

    fn update_node(sender: Sender<StateResponseMsg>,
                   state: &State,
                   node_name: String,
                   node_type: String,
                   node_function: String,
                   slave_id: String) {
        state.node_list.update_node(node_name.clone(),
                                    node_type.clone(),
                                    node_function.clone(),
                                    slave_id.clone());
        let msg = StateResponseMsg::UpdateNode;
        sender.send(msg).unwrap();
    }

    fn set_node_inactive(sender: Sender<StateResponseMsg>, state: &State, node_name: String) {
        state.node_list.set_node_inactive(node_name.clone());
        let msg = StateResponseMsg::SetNodeInactive;
        sender.send(msg).unwrap();
    }

    fn get_node(sender: Sender<StateResponseMsg>, state: &State, node_name: String) {
        let result: Node = state.node_list.get_node(node_name.clone()).unwrap();
        let msg = StateResponseMsg::GetNode { node: result };
        sender.send(msg).unwrap();
    }

    fn get_nodes(sender: Sender<StateResponseMsg>, state: &State) {
        let result: Vec<Node> = state.node_list.get_nodes();
        let msg = StateResponseMsg::GetNodes { nodes: result };
        sender.send(msg).unwrap();
    }
}
