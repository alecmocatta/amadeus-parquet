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

//! Implement [`Record`] for [`List`].

use std::{
    collections::HashMap,
    fmt::{self, Debug},
    ops::Index,
    slice::{self, SliceIndex},
    vec,
};

use crate::{
    basic::{LogicalType, Repetition},
    column::reader::ColumnReader,
    errors::{ParquetError, Result},
    record::{
        reader::{MapReader, RepeatedReader},
        schemas::{ListSchema, ListSchemaType},
        Reader, Record,
    },
    schema::types::{ColumnPath, Type},
};

/// Returns true if repeated type is an element type for the list.
/// Used to determine legacy list types.
/// This method is copied from Spark Parquet reader and is based on the reference:
/// https://github.com/apache/parquet-format/blob/master/LogicalTypes.md#backward-compatibility-rules
pub(super) fn parse_list<T: Record>(schema: &Type) -> Result<ListSchema<T::Schema>> {
    if schema.is_group()
        && schema.get_basic_info().logical_type() == LogicalType::List
        && schema.get_fields().len() == 1
    {
        let sub_schema = schema.get_fields().into_iter().nth(0).unwrap();
        if sub_schema.get_basic_info().repetition() == Repetition::Repeated {
            return Ok(
                if sub_schema.is_group()
                    && sub_schema.get_fields().len() == 1
                    && sub_schema.name() != "array"
                    && sub_schema.name() != format!("{}_tuple", schema.name())
                {
                    let element = sub_schema.get_fields().into_iter().nth(0).unwrap();
                    let list_name = if sub_schema.name() == "list" {
                        None
                    } else {
                        Some(sub_schema.name().to_owned())
                    };
                    let element_name = if element.name() == "element" {
                        None
                    } else {
                        Some(element.name().to_owned())
                    };

                    ListSchema(
                        T::parse(&*element, Some(element.get_basic_info().repetition()))?
                            .1,
                        ListSchemaType::List(list_name, element_name),
                    )
                } else {
                    let element_name = sub_schema.name().to_owned();
                    ListSchema(
                        T::parse(&*sub_schema, Some(Repetition::Repeated))?.1,
                        ListSchemaType::ListCompat(element_name),
                    )
                },
            );
        }
    }
    Err(ParquetError::General(String::from(
        "Couldn't parse List<T>",
    )))
}

/// `List<T>` corresponds to the [List logical type](https://github.com/apache/parquet-format/blob/master/LogicalTypes.md#lists).
#[derive(Clone, Hash, Eq)]
pub struct List<T>(pub(in super::super) Vec<T>);

impl<T> Record for List<T>
where
    T: Record,
{
    type Schema = ListSchema<T::Schema>;
    type Reader = impl Reader<Item = Self>;

    fn parse(
        schema: &Type,
        repetition: Option<Repetition>,
    ) -> Result<(String, Self::Schema)> {
        if repetition == Some(Repetition::Required) {
            return parse_list::<T>(schema)
                .map(|schema2| (schema.name().to_owned(), schema2));
        }
        // A repeated field that is neither contained by a `LIST`- or `MAP`-annotated
        // group nor annotated by `LIST` or `MAP` should be interpreted as a
        // required list of required elements where the element type is the type
        // of the field.
        if repetition == Some(Repetition::Repeated) {
            return Ok((
                schema.name().to_owned(),
                ListSchema(
                    T::parse(&schema, Some(Repetition::Required))?.1,
                    ListSchemaType::Repeated,
                ),
            ));
        }
        Err(ParquetError::General(String::from(
            "Couldn't parse List<T>",
        )))
    }

    fn reader(
        schema: &Self::Schema,
        path: &mut Vec<String>,
        def_level: i16,
        rep_level: i16,
        paths: &mut HashMap<ColumnPath, ColumnReader>,
        batch_size: usize,
    ) -> Self::Reader {
        MapReader(
            match schema.1 {
                ListSchemaType::List(ref list_name, ref element_name) => {
                    let list_name = list_name.as_ref().map(|x| &**x).unwrap_or("list");
                    let element_name =
                        element_name.as_ref().map(|x| &**x).unwrap_or("element");

                    path.push(list_name.to_owned());
                    path.push(element_name.to_owned());
                    let reader = T::reader(
                        &schema.0,
                        path,
                        def_level + 1,
                        rep_level + 1,
                        paths,
                        batch_size,
                    );
                    let _ = path.pop().unwrap();
                    let _ = path.pop().unwrap();

                    RepeatedReader { reader }
                }
                ListSchemaType::ListCompat(ref element_name) => {
                    path.push(element_name.to_owned());
                    let reader = T::reader(
                        &schema.0,
                        path,
                        def_level + 1,
                        rep_level + 1,
                        paths,
                        batch_size,
                    );
                    let _ = path.pop().unwrap();

                    RepeatedReader { reader }
                }
                ListSchemaType::Repeated => {
                    let reader = T::reader(
                        &schema.0,
                        path,
                        def_level + 1,
                        rep_level + 1,
                        paths,
                        batch_size,
                    );
                    RepeatedReader { reader }
                }
            },
            |x| Ok(List(x)),
        )
    }
}

impl<T> List<T> {
    /// Returns an iterator over references to the elements of the List.
    pub fn iter(&self) -> slice::Iter<'_, T> {
        self.0.iter()
    }

    /// Creates an iterator over the elements of the List.
    pub fn into_iter(self) -> vec::IntoIter<T> {
        self.0.into_iter()
    }
}
impl<T> From<Vec<T>> for List<T> {
    fn from(vec: Vec<T>) -> Self {
        List(vec)
    }
}
impl<T> Into<Vec<T>> for List<T> {
    fn into(self) -> Vec<T> {
        self.0
    }
}
impl<T, U> PartialEq<List<U>> for List<T>
where
    T: PartialEq<U>,
{
    fn eq(&self, other: &List<U>) -> bool {
        self.0 == other.0
    }
}
impl<T, I> Index<I> for List<T>
where
    I: SliceIndex<[T]>,
{
    type Output = <I as SliceIndex<[T]>>::Output;

    fn index(&self, index: I) -> &Self::Output {
        self.0.index(index)
    }
}
impl<T> Debug for List<T>
where
    T: Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_list().entries(self.iter()).finish()
    }
}
