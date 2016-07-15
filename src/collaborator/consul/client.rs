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

use state::Task;

use hyper::Client;


lazy_static! {
    static ref CLIENT: Client = Client::new();
}

pub fn register_running_task(master_ip: &String, task: &Task) {
    register_service(master_ip, task);
}

pub fn register_torc_controller(master_ip: &String, controller_name: &String, controller_ip: &String) {
    register_controller(master_ip, controller_name, controller_ip);
}

pub fn register_unmanaged_service(master_ip: &String, service_name: &String, service_ip: &String) {
    register_controller(master_ip, service_name, service_ip);
}

fn register_controller(master_ip: &String, controller_name: &String, controller_ip: &String) {
    register(master_ip, controller_name, controller_ip);
}

fn register_service(master_ip: &String, task: &Task) {
    register(master_ip, &task.name, &task.ip);
}

fn register(master_ip: &String, name: &String, ip: &String) {
    let address = format!("http://{}:8500/v1/agent/service/register", master_ip);

    let service_description = format!{"{{\"Name\": \"{}\",\"Address\": \"{}\"}}", name, ip};
    let _ = CLIENT.post(&address).body(&service_description).send();
}
