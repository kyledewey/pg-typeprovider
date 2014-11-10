#![crate_type="dylib"]
#![feature(plugin_registrar, phase)]

extern crate syntax;
extern crate rustc;
extern crate postgres;
extern crate time;
// extern crate regex;
// #[phase(plugin)] extern crate regex_macros;

use syntax::codemap::{Span, Spanned};
use syntax::parse::token;
use syntax::ast::{TokenTree, TtToken};
use syntax::ast;
use syntax::ext::base::{ExtCtxt, MacResult, DummyResult, MacItems};
use syntax::ext::build::AstBuilder;  // trait for expr_uint
use syntax::ptr::P;
use syntax::parse::token::intern;
use rustc::plugin::Registry;

use std::collections::HashMap;
use postgres::{Connection, NoSsl};

fn expand_pg_table(cx: &mut ExtCtxt, sp: Span, args: &[TokenTree])
        -> Box<MacResult + 'static> {

    // Get the table name.
    let table_name = match args {
        [TtToken(_, token::Ident(s, _))] => token::get_ident(s).to_string(),
        _ => {
            cx.span_err(sp, "argument should be a single identifier");
            return DummyResult::any(sp);
        }
    };

    // Open a connection to PG.
    let conn = Connection::connect("postgres://jroesch@localhost/gradr-production", &NoSsl)
            .unwrap();

    let schema: HashMap<String, PgType> = schema_for(conn, &table_name);

    // for (name, ty) in schema.iter() {
    //     debug!("FieldName: {}; Type: {}", name, ty)
    // }

    let struct_def = struct_def_for(cx, sp, schema);

    let struct_name = structify(&table_name);

    let struct_item = cx.item_struct(sp, struct_name, struct_def);
    MacItems::new(vec![struct_item].into_iter())
}

#[deriving(Show, PartialEq)]
enum PgType {
    PgInt,
    PgBool,
    PgString,
    PgTime
}

impl PgType {
    pub fn from_data_type(s: &String) -> PgType {
        match s.as_slice() {
            "integer" => PgInt,
            "boolean" => PgBool,
            "character varying" => PgString,
            "timestamp without time zone" => PgTime,
            s => panic!("type {} not yet supported", s)
        }
    }

    pub fn to_rust_type(&self, cx: &mut ExtCtxt, sp: Span) -> P<ast::Ty> {
        match self {
            &PgInt => cx.ty_ident(sp, ast::Ident::new(intern("int"))),
            &PgBool => cx.ty_ident(sp, ast::Ident::new(intern("bool"))),
            &PgString => cx.ty_ident(sp, ast::Ident::new(intern("String"))),
            &PgTime => cx.ty_ident(sp, ast::Ident::new(intern("Timespec")))
        }
    }
}

fn schema_for(conn: Connection, table_name: &String) -> HashMap<String, PgType> {
    let query = format!("select column_name, \
                                data_type, \
                                character_maximum_length \
                         from INFORMATION_SCHEMA.COLUMNS \
                         where table_name = '{}';", table_name);

    let rows = conn.prepare(query.as_slice()).unwrap();

    let mut schema = HashMap::new();

    for row in rows.query([]).unwrap() {
        schema.insert(row.get(0), PgType::from_data_type(&row.get(1)));
    }

    schema
}

fn struct_def_for(ecx: &mut ExtCtxt, span: Span, schema: HashMap<String, PgType>) -> ast::StructDef {
    let mut fields = Vec::new();

    for (name, ty) in schema.into_iter() {
        fields.push(struct_field_for(ecx, span, name, ty))
    }

    ast::StructDef { fields: fields, ctor_id: None }
}

fn struct_field_for(ecx: &mut ExtCtxt, sp: Span, field_name: String, ty: PgType) -> ast::StructField {
    let struct_field_ = ast::StructField_ {
        kind: ast::NamedField(ast::Ident::new(intern(field_name.as_slice())), ast::Public),
        id: ast::DUMMY_NODE_ID,
        ty: ty.to_rust_type(ecx, sp),
        attrs: Vec::new()
    };

    Spanned { node: struct_field_, span: sp }
}

// de-plural and capitialize name (ActiveRecord name convention)
fn structify(name: &String) -> ast::Ident {
    let name: String = name.as_slice().chars().enumerate().filter_map(|(i, c)| {
        if i == 0 {
            Some(c.to_uppercase())
        } else if i == name.len() - 1 {
            None
        } else {
            Some(c.clone())
        }
    }).collect();

    ast::Ident::new(intern(name.as_slice()))
}

#[plugin_registrar]
pub fn plugin_registrar(reg: &mut Registry) {
    reg.register_macro("pg_table", expand_pg_table);
}