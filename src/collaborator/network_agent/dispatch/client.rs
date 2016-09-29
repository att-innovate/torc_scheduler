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

use super::super::fboss;
use super::super::snaproute;

pub fn reset_fib(agent_type: &String, connection: &String) {
    println!("reset_fib [{}] [{}]", agent_type, connection);
    match agent_type.as_str() {
        "fboss" => fboss::reset_fib(&connection),
        "snaproute" => snaproute::reset_fib(&connection),
        "undefined" => println!("network-agent undefined"),
        _ => println!("!! network-agent type {} unknown!!", agent_type),
    }
}

pub fn add_route(agent_type: &String, connection: &String, route_to: &String, route_via: &String) {
    println!("add route {}, {}, {}, {}",
             agent_type,
             connection,
             route_to,
             route_via);

    if route_via.is_empty() {
        return;
    }
    if connection.starts_with(route_to) {
        return;
    }

    let route_to = format!("{}/32", route_to.clone());

    match agent_type.as_str() {
        "fboss" => fboss::add_route(&connection, &route_to, &route_via),
        "snaproute" => snaproute::add_route(&connection, &route_to, &route_via),
        _ => println!("!! network-agent type {} unknown!!", agent_type),
    }
}

pub fn delete_route(agent_type: &String, connection: &String, route_to: &String) {
    println!("delete route {}, {}, {}", agent_type, connection, route_to);

    if route_to.is_empty() {
        return;
    }
    let route_to = format!("{}/32", route_to.clone());

    match agent_type.as_str() {
        "fboss" => fboss::delete_route(&connection, &route_to),
        "snaproute" => snaproute::delete_route(&connection, &route_to),
        _ => println!("!! network-agent type {} unknown!!", agent_type),
    }
}
