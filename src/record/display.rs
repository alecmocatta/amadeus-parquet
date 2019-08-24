// Licensed to the Apache Software Foundation (ASF) under one
// or more contributor license agreements.  See the NOTICE file
// distributed with this work for additional information
// regarding copyright ownership.  The ASF licenses this file
// to you under the Apache License, Version 2.0 (the
// "License"); you may not use this file except in compliance
// with the License.  You may obtain a copy of the License at
//
//   http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing,
// software distributed under the License is distributed on an
// "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
// KIND, either express or implied.  See the License for the
// specific language governing permissions and limitations
// under the License.

//! Utility structs and methods to help with displaying schemas

use std::fmt::{self, Display, Write};

use super::Schema;
use crate::basic::{LogicalType, Repetition};

/// Implement [`Display`] given a closure that accepts a [`fmt::Formatter`] and returns a
/// [`fmt::Result`].
pub struct DisplayFmt<F>(F)
where
    F: Fn(&mut fmt::Formatter) -> fmt::Result;

impl<F> DisplayFmt<F>
where
    F: Fn(&mut fmt::Formatter) -> fmt::Result,
{
    pub fn new(f: F) -> Self {
        Self(f)
    }
}
impl<F> Display for DisplayFmt<F>
where
    F: Fn(&mut fmt::Formatter) -> fmt::Result,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.0(f)
    }
}

// The following is code adapted from https://github.com/rust-lang/rust/blob/79d8a0fcefa5134db2a94739b1d18daa01fc6e9f/src/libcore/fmt/builders.rs

struct PadAdapter<T: Write> {
    fmt: T,
    on_newline: bool,
}

impl<T> PadAdapter<T>
where
    T: Write,
{
    fn new(fmt: T) -> Self {
        PadAdapter {
            fmt,
            on_newline: false,
        }
    }
}

impl<T> Write for PadAdapter<T>
where
    T: Write,
{
    fn write_str(&mut self, mut s: &str) -> fmt::Result {
        while !s.is_empty() {
            if self.on_newline {
                self.fmt.write_str("    ")?;
            }

            let split = match s.find('\n') {
                Some(pos) => {
                    self.on_newline = true;
                    pos + 1
                }
                None => {
                    self.on_newline = false;
                    s.len()
                }
            };
            self.fmt.write_str(&s[..split])?;
            s = &s[split..];
        }

        Ok(())
    }
}

/// Display a group. Tuples, [`Group`](super::types::Group) and structs marked with
/// `#[derive(Record)]` all make use of this, so the logic is encapsulated here.
#[must_use = "must eventually call `finish()` on Debug builders"]
#[allow(missing_debug_implementations)]
pub struct DisplaySchemaGroup<'a, 'b: 'a> {
    fmt: &'a mut fmt::Formatter<'b>,
    result: fmt::Result,
    has_fields: bool,
}

impl<'a, 'b: 'a> DisplaySchemaGroup<'a, 'b> {
    pub fn new(
        r: Option<Repetition>,
        name: Option<&str>,
        logical: Option<LogicalType>,
        fmt: &'a mut fmt::Formatter<'b>,
    ) -> Self {
        let mut result = if let Some(r) = r {
            fmt.write_str(&r.to_string())
                .and_then(|_| fmt.write_str(" group "))
        } else {
            fmt.write_str("message ")
        };
        result = result.and_then(|_| fmt.write_str(name.unwrap_or("<name>")));
        if let Some(logical) = logical {
            result = result
                .and_then(|_| fmt.write_str(" ("))
                .and_then(|_| fmt.write_str(&logical.to_string()))
                .and_then(|_| fmt.write_str(")"));
        }
        result = result.and_then(|_| fmt.write_str(" {"));
        DisplaySchemaGroup {
            fmt,
            result,
            has_fields: false,
        }
    }

    /// Adds a new field to the generated struct output.
    pub fn field(
        &mut self,
        name: Option<&str>,
        value: Option<&impl Schema>,
    ) -> &mut DisplaySchemaGroup<'a, 'b> {
        self.result = self.result.and_then(|_| {
            let mut writer = PadAdapter::new(&mut self.fmt);
            writer.write_fmt(format_args!(
                "\n{}",
                DisplayFmt::new(|fmt| Schema::fmt(
                    value,
                    Some(Repetition::Required),
                    name,
                    fmt
                ))
            ))
        });

        self.has_fields = true;
        self
    }

    /// Finishes output and returns any error encountered.
    pub fn finish(&mut self) -> fmt::Result {
        if self.has_fields {
            self.result = self.result.and_then(|_| self.fmt.write_str("\n}"));
        } else {
            self.result = self.result.and_then(|_| self.fmt.write_str(" }"));
        }
        self.result
    }
}
