use model::{BinaryOp, UnaryOp, Type, GlobalVar as AstGlobalVar};

/// Variable identifier in IR
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct VarId(pub usize);

/// Basic block identifier in IR
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct BlockId(pub usize);

/// Operand in IR instructions - can be a constant, variable, or global
#[derive(Debug, Clone)]
pub enum Operand {
    Constant(i64),
    Var(VarId),
    Global(String),
}

/// IR Instructions in SSA form
#[derive(Debug, Clone)]
pub enum Instruction {
    Binary {
        dest: VarId,
        op: BinaryOp,
        left: Operand,
        right: Operand,
    },
    Unary {
        dest: VarId,
        op: UnaryOp,
        src: Operand,
    },
    Phi {
        dest: VarId,
        // (BlockId where the value comes from, VarId)
        preds: Vec<(BlockId, VarId)>,
    },
    Copy {
        dest: VarId,
        src: Operand,
    },
    Alloca {
        dest: VarId,
        r#type: Type,
    },
    Load {
        dest: VarId,
        addr: Operand,
    },
    Store {
        addr: Operand,
        src: Operand,
    },
    GetElementPtr {
        dest: VarId,
        base: Operand,
        index: Operand,
        element_type: Type,
    },
    Call {
        dest: Option<VarId>,
        name: String,
        args: Vec<Operand>,
    },
    IndirectCall {
        dest: Option<VarId>,
        func_ptr: Operand,
        args: Vec<Operand>,
    },
}

/// Control flow terminators for basic blocks
#[derive(Debug, Clone)]
pub enum Terminator {
    Br(BlockId),
    CondBr {
        cond: Operand,
        then_block: BlockId,
        else_block: BlockId,
    },
    Ret(Option<Operand>),
    Unreachable,
}

/// Basic block with instructions and terminator
#[derive(Debug, Clone)]
pub struct BasicBlock {
    pub id: BlockId,
    pub instructions: Vec<Instruction>,
    pub terminator: Terminator,
}

/// Function in IR form
#[derive(Debug, Clone)]
pub struct Function {
    pub name: String,
    pub return_type: Type,
    pub params: Vec<(Type, VarId)>,
    pub blocks: Vec<BasicBlock>,
    pub entry_block: BlockId,
}

/// Complete IR program
#[derive(Debug, Clone)]
pub struct IRProgram {
    pub functions: Vec<Function>,
    pub global_strings: Vec<(String, String)>, // (label, content)
    pub globals: Vec<AstGlobalVar>,
    pub structs: Vec<model::StructDef>,
}
