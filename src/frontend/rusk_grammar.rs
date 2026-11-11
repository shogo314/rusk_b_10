use crate::frontend::{
    rusk_grammar_trait::{self as g, Rusk, RuskGrammarTrait},
    text::{Declaration, LayoutHints},
};
#[derive(Default)]
pub struct RuskGrammar<'t> {
    pub root: Option<Rusk<'t>>,
    pub collect_layout: bool,
    pub layout: LayoutHints,
    pub syntax_error: Option<String>,
}
impl<'t> RuskGrammarTrait<'t> for RuskGrammar<'t> {
    fn rusk(&mut self, arg: &Rusk<'t>) -> parol_runtime::Result<()> {
        if self.syntax_error.is_none() {
            self.root = Some(arg.clone());
        }
        Ok(())
    }

    fn type_parameters(&mut self, arg: &g::TypeParameters<'t>) -> parol_runtime::Result<()> {
        if self.collect_layout {
            self.layout
                .type_starts
                .insert(arg.identifier.identifier.location.start());
        }
        Ok(())
    }
    fn type_arguments(&mut self, arg: &g::TypeArguments<'t>) -> parol_runtime::Result<()> {
        fn start(ty: &g::TypeName<'_>) -> usize {
            match ty {
                g::TypeName::NamedType(n) => n.named_type.identifier.identifier.location.start(),
                g::TypeName::TupleType(t) => start(&t.tuple_type.type_name),
            }
        }
        if self.collect_layout {
            self.layout.type_starts.insert(start(&arg.type_name));
        }
        Ok(())
    }
    fn unary_op(&mut self, arg: &g::UnaryOp<'t>) -> parol_runtime::Result<()> {
        if self.collect_layout {
            let token = super::ast::unary_token(arg);
            self.layout.unary.insert(token.location.start());
        }
        Ok(())
    }
    fn left_angle_op(&mut self, arg: &g::LeftAngleOp<'t>) -> parol_runtime::Result<()> {
        if let Some(tail) = &arg.left_angle_op_opt {
            self.shift_operator(&arg.l_t, &tail.l_t);
        }
        Ok(())
    }
    fn right_angle_op(&mut self, arg: &g::RightAngleOp<'t>) -> parol_runtime::Result<()> {
        if let Some(tail) = &arg.right_angle_op_opt {
            self.shift_operator(&arg.g_t, &tail.g_t);
        }
        Ok(())
    }
    fn negative_pattern(&mut self, arg: &g::NegativePattern<'t>) -> parol_runtime::Result<()> {
        if self.collect_layout {
            self.layout.negative_patterns.insert(
                super::ast::number_literal(&arg.number_literal)
                    .1
                    .location
                    .start(),
            );
        }
        Ok(())
    }
    fn block(&mut self, arg: &g::Block<'t>) -> parol_runtime::Result<()> {
        // BlockItems is recursive in the CST to distinguish a final value
        // expression from a statement. The generated CST clone is recursive.
        const MAX_BLOCK_ITEMS: usize = 1024;
        let mut items = arg.block_items.as_ref();
        let mut statements = 0;
        loop {
            items = match items {
                g::BlockItems::SimpleStatementBlockItems(item) => &item.block_items,
                g::BlockItems::BlockStatementBlockItems(item) => &item.block_items,
                g::BlockItems::StatementExprBlockTail(item) => match item.block_tail.as_ref() {
                    g::BlockTail::SemicolonBlockItems(tail) => &tail.block_items,
                    g::BlockTail::BlockTailEmpty(_) => break,
                },
                g::BlockItems::BlockItemsEmpty(_) => break,
            };
            statements += 1;
            if statements > MAX_BLOCK_ITEMS {
                self.syntax_error.get_or_insert_with(|| {
                    super::ast::Span::token(&arg.l_brace).error(format!(
                        "block statement limit ({MAX_BLOCK_ITEMS}) exceeded"
                    ))
                });
                break;
            }
        }
        if self.collect_layout {
            self.layout.blocks.insert(arg.l_brace.location.start());
        }
        Ok(())
    }
    fn index(&mut self, arg: &g::Index<'t>) -> parol_runtime::Result<()> {
        if self.collect_layout {
            self.layout.indexes.insert(arg.l_bracket.location.start());
        }
        Ok(())
    }
    fn function(&mut self, arg: &g::Function<'t>) -> parol_runtime::Result<()> {
        self.declaration("function", &arg.identifier);
        Ok(())
    }
    fn extern_fn(&mut self, arg: &g::ExternFn<'t>) -> parol_runtime::Result<()> {
        self.declaration("extern function", &arg.identifier);
        Ok(())
    }
    fn struct_def(&mut self, arg: &g::StructDef<'t>) -> parol_runtime::Result<()> {
        self.declaration("struct", &arg.identifier);
        Ok(())
    }
    fn struct_field(&mut self, arg: &g::StructField<'t>) -> parol_runtime::Result<()> {
        self.declaration("field", &arg.identifier);
        Ok(())
    }
    fn enum_def(&mut self, arg: &g::EnumDef<'t>) -> parol_runtime::Result<()> {
        self.declaration("enum", &arg.identifier);
        Ok(())
    }
    fn enum_variant(&mut self, arg: &g::EnumVariant<'t>) -> parol_runtime::Result<()> {
        self.declaration("variant", &arg.identifier);
        Ok(())
    }
    fn extern_symbol(&mut self, arg: &g::ExternSymbol<'t>) -> parol_runtime::Result<()> {
        self.declaration("extern symbol", &arg.identifier);
        Ok(())
    }
    fn extern_type(&mut self, arg: &g::ExternType<'t>) -> parol_runtime::Result<()> {
        self.declaration("extern type", first_type_identifier(&arg.type_name));
        Ok(())
    }
}

fn first_type_identifier<'a, 't>(mut ty: &'a g::TypeName<'t>) -> &'a g::Identifier<'t> {
    loop {
        match ty {
            g::TypeName::NamedType(named) => return &named.named_type.identifier,
            g::TypeName::TupleType(tuple) => ty = &tuple.tuple_type.type_name,
        }
    }
}

impl RuskGrammar<'_> {
    fn shift_operator(
        &mut self,
        first: &parol_runtime::Token<'_>,
        second: &parol_runtime::Token<'_>,
    ) {
        // Lex the two angle brackets separately so nested type arguments
        // keep the same closing tokens.
        if first.location.end() != second.location.start() {
            self.syntax_error.get_or_insert_with(|| {
                super::ast::Span::token(first)
                    .error("shift operator must be written without whitespace")
            });
        } else if self.collect_layout {
            self.layout.joint_operators.insert(second.location.start());
        }
    }
    fn declaration(&mut self, kind: &'static str, name: &g::Identifier<'_>) {
        if self.collect_layout {
            self.layout.declarations.push(Declaration {
                kind,
                name: name.identifier.text().to_string(),
                name_offset: name.identifier.location.start(),
                line: name.identifier.location.start_line,
                column: name.identifier.location.start_column,
            });
        }
    }
}
