//! ERL program parsing and execution.
//!
//! Programs consist of instructions (SET, IF/ELSE/ENDIF, WHILE/ENDWHILE,
//! RETURN, RUN) that operate on ERL variables through expressions.
//! Expressions support arithmetic, comparison, logical, and built-in
//! math/psychrometric functions.

use crate::variable::{ErlValue, VariableManager};
use ep_core::plugin::CallingPoint;

/// ERL statement keyword.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErlKeyword {
    Set,
    If,
    ElseIf,
    Else,
    EndIf,
    While,
    EndWhile,
    Return,
    Run,
    Goto,
}

/// A single ERL instruction.
#[derive(Debug, Clone)]
pub struct ErlInstruction {
    pub keyword: ErlKeyword,
    /// Variable index for SET (LHS) or subroutine index for RUN.
    pub arg1: usize,
    /// Expression index or goto target.
    pub arg2: usize,
}

/// ERL expression operator / built-in function.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErlOp {
    // Arithmetic
    Add, Subtract, Multiply, Divide, Negate, Power,
    // Comparison
    Equal, NotEqual, LessThan, GreaterThan, LessOrEqual, GreaterOrEqual,
    // Logical
    And, Or,
    // Math functions
    Round, Mod, Abs, Exp, Ln, Sqrt,
    Sin, Cos, ArcSin, ArcCos, DegToRad, RadToDeg,
    Min, Max,
    // Psychrometric
    CpAir, Enthalpy, TdbFromEnthalpyW, RhoAir,
    // Variable/literal reference
    Literal(usize),   // index into literals array
    Variable(usize),  // index into variable manager
    // Trend access
    TrendValue, TrendAverage, TrendMax, TrendMin, TrendSum, TrendDirection,
    // Curve evaluation
    CurveValue,
}

/// An expression node in the expression tree.
#[derive(Debug, Clone)]
pub struct ErlExpression {
    pub op: ErlOp,
    pub operands: Vec<usize>, // indices into expression array (for sub-expressions)
}

/// A complete ERL program (or subroutine).
#[derive(Debug, Clone)]
pub struct ErlProgram {
    pub name: String,
    pub instructions: Vec<ErlInstruction>,
    pub expressions: Vec<ErlExpression>,
    pub literals: Vec<f64>,
}

impl ErlProgram {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            instructions: Vec::new(),
            expressions: Vec::new(),
            literals: Vec::new(),
        }
    }

    /// Add a literal value, returning its index.
    pub fn add_literal(&mut self, value: f64) -> usize {
        let idx = self.literals.len();
        self.literals.push(value);
        idx
    }

    /// Add an expression, returning its index.
    pub fn add_expression(&mut self, expr: ErlExpression) -> usize {
        let idx = self.expressions.len();
        self.expressions.push(expr);
        idx
    }

    /// Execute the program, modifying variables.
    pub fn execute(&self, vars: &mut VariableManager) {
        let mut pc = 0;
        let mut iteration_count = 0u32;
        let max_iterations = 1_000_000;

        while pc < self.instructions.len() {
            let inst = &self.instructions[pc];
            match inst.keyword {
                ErlKeyword::Set => {
                    let value = self.evaluate_expression(inst.arg2, vars);
                    vars.set(inst.arg1, value);
                    pc += 1;
                }
                ErlKeyword::If | ErlKeyword::ElseIf => {
                    let cond = self.evaluate_expression(inst.arg2, vars);
                    if cond.is_true() {
                        pc += 1; // Enter the block
                    } else {
                        pc = inst.arg1; // Jump to else/elseif/endif (stored in arg1 as goto)
                    }
                }
                ErlKeyword::Else => {
                    pc += 1; // Always enter else block (reached by falling through)
                }
                ErlKeyword::EndIf => {
                    pc += 1; // Continue past endif
                }
                ErlKeyword::While => {
                    let cond = self.evaluate_expression(inst.arg2, vars);
                    if cond.is_true() {
                        pc += 1;
                    } else {
                        pc = inst.arg1; // Jump past EndWhile
                    }
                    iteration_count += 1;
                    if iteration_count > max_iterations {
                        break; // Prevent infinite loops
                    }
                }
                ErlKeyword::EndWhile => {
                    pc = inst.arg1; // Jump back to While
                }
                ErlKeyword::Return => {
                    break;
                }
                ErlKeyword::Goto => {
                    pc = inst.arg1;
                }
                ErlKeyword::Run => {
                    // Subroutine calls not supported in simplified model
                    pc += 1;
                }
            }
        }
    }

    /// Evaluate an expression tree, returning a value.
    fn evaluate_expression(&self, expr_idx: usize, vars: &VariableManager) -> ErlValue {
        if expr_idx >= self.expressions.len() {
            return ErlValue::Null;
        }
        let expr = &self.expressions[expr_idx];

        match expr.op {
            ErlOp::Literal(lit_idx) => {
                ErlValue::Number(self.literals.get(lit_idx).copied().unwrap_or(0.0))
            }
            ErlOp::Variable(var_idx) => {
                vars.get(var_idx).cloned().unwrap_or(ErlValue::Null)
            }
            ErlOp::Add => self.binary_op(expr, vars, |a, b| a + b),
            ErlOp::Subtract => self.binary_op(expr, vars, |a, b| a - b),
            ErlOp::Multiply => self.binary_op(expr, vars, |a, b| a * b),
            ErlOp::Divide => {
                let a = self.evaluate_expression(expr.operands[0], vars).as_number();
                let b = self.evaluate_expression(expr.operands[1], vars).as_number();
                if b.abs() < 1e-30 {
                    ErlValue::Error("Divide by zero".into())
                } else {
                    ErlValue::Number(a / b)
                }
            }
            ErlOp::Negate => {
                let a = self.evaluate_expression(expr.operands[0], vars).as_number();
                ErlValue::Number(-a)
            }
            ErlOp::Power => {
                let a = self.evaluate_expression(expr.operands[0], vars).as_number();
                let b = self.evaluate_expression(expr.operands[1], vars).as_number();
                let result = a.powf(b);
                if result.is_nan() {
                    ErlValue::Error("Power resulted in NaN".into())
                } else {
                    ErlValue::Number(result)
                }
            }
            ErlOp::Equal => self.compare_op(expr, vars, |a, b| (a - b).abs() < 1e-12),
            ErlOp::NotEqual => self.compare_op(expr, vars, |a, b| (a - b).abs() >= 1e-12),
            ErlOp::LessThan => self.compare_op(expr, vars, |a, b| a < b),
            ErlOp::GreaterThan => self.compare_op(expr, vars, |a, b| a > b),
            ErlOp::LessOrEqual => self.compare_op(expr, vars, |a, b| a <= b),
            ErlOp::GreaterOrEqual => self.compare_op(expr, vars, |a, b| a >= b),
            ErlOp::And => {
                let a = self.evaluate_expression(expr.operands[0], vars).is_true();
                let b = self.evaluate_expression(expr.operands[1], vars).is_true();
                ErlValue::Number(if a && b { 1.0 } else { 0.0 })
            }
            ErlOp::Or => {
                let a = self.evaluate_expression(expr.operands[0], vars).is_true();
                let b = self.evaluate_expression(expr.operands[1], vars).is_true();
                ErlValue::Number(if a || b { 1.0 } else { 0.0 })
            }
            ErlOp::Round => self.unary_op(expr, vars, |a| a.round()),
            ErlOp::Mod => self.binary_op(expr, vars, |a, b| a % b),
            ErlOp::Abs => self.unary_op(expr, vars, |a| a.abs()),
            ErlOp::Exp => self.unary_op(expr, vars, |a| a.exp().min(1e100)),
            ErlOp::Ln => {
                let a = self.evaluate_expression(expr.operands[0], vars).as_number();
                if a <= 0.0 {
                    ErlValue::Error("Ln of non-positive".into())
                } else {
                    ErlValue::Number(a.ln())
                }
            }
            ErlOp::Sqrt => {
                let a = self.evaluate_expression(expr.operands[0], vars).as_number();
                if a < 0.0 {
                    ErlValue::Error("Sqrt of negative".into())
                } else {
                    ErlValue::Number(a.sqrt())
                }
            }
            ErlOp::Sin => self.unary_op(expr, vars, |a| a.sin()),
            ErlOp::Cos => self.unary_op(expr, vars, |a| a.cos()),
            ErlOp::ArcSin => self.unary_op(expr, vars, |a| a.asin()),
            ErlOp::ArcCos => self.unary_op(expr, vars, |a| a.acos()),
            ErlOp::DegToRad => self.unary_op(expr, vars, |a| a.to_radians()),
            ErlOp::RadToDeg => self.unary_op(expr, vars, |a| a.to_degrees()),
            ErlOp::Min => self.binary_op(expr, vars, f64::min),
            ErlOp::Max => self.binary_op(expr, vars, f64::max),
            ErlOp::CpAir => {
                let w = self.evaluate_expression(expr.operands[0], vars).as_number();
                ErlValue::Number(ep_psychrometrics::cp_air(w))
            }
            ErlOp::Enthalpy => {
                let t = self.evaluate_expression(expr.operands[0], vars).as_number();
                let w = self.evaluate_expression(expr.operands[1], vars).as_number();
                ErlValue::Number(ep_psychrometrics::enthalpy(t, w))
            }
            ErlOp::TdbFromEnthalpyW => {
                let h = self.evaluate_expression(expr.operands[0], vars).as_number();
                let w = self.evaluate_expression(expr.operands[1], vars).as_number();
                ErlValue::Number(ep_psychrometrics::t_db_from_enthalpy_w(h, w))
            }
            ErlOp::RhoAir => {
                let t = self.evaluate_expression(expr.operands[0], vars).as_number();
                let w = self.evaluate_expression(expr.operands[1], vars).as_number();
                let p = if expr.operands.len() > 2 {
                    self.evaluate_expression(expr.operands[2], vars).as_number()
                } else {
                    101325.0
                };
                ErlValue::Number(ep_psychrometrics::rho_air(p, t, w))
            }
            // Trend and curve operations need external state, return Null in basic eval
            ErlOp::TrendValue | ErlOp::TrendAverage | ErlOp::TrendMax
            | ErlOp::TrendMin | ErlOp::TrendSum | ErlOp::TrendDirection
            | ErlOp::CurveValue => ErlValue::Null,
        }
    }

    fn unary_op(&self, expr: &ErlExpression, vars: &VariableManager, f: impl Fn(f64) -> f64) -> ErlValue {
        let a = self.evaluate_expression(expr.operands[0], vars).as_number();
        ErlValue::Number(f(a))
    }

    fn binary_op(&self, expr: &ErlExpression, vars: &VariableManager, f: impl Fn(f64, f64) -> f64) -> ErlValue {
        let a = self.evaluate_expression(expr.operands[0], vars).as_number();
        let b = self.evaluate_expression(expr.operands[1], vars).as_number();
        ErlValue::Number(f(a, b))
    }

    fn compare_op(&self, expr: &ErlExpression, vars: &VariableManager, f: impl Fn(f64, f64) -> bool) -> ErlValue {
        let a = self.evaluate_expression(expr.operands[0], vars).as_number();
        let b = self.evaluate_expression(expr.operands[1], vars).as_number();
        ErlValue::Number(if f(a, b) { 1.0 } else { 0.0 })
    }
}

/// A program manager that associates programs with a calling point.
#[derive(Debug, Clone)]
pub struct ProgramManager {
    pub name: String,
    pub calling_point: CallingPoint,
    pub program_indices: Vec<usize>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper to build a simple SET program: SET var = expr
    fn build_set_program(vars: &mut VariableManager, var_name: &str, value: f64) -> ErlProgram {
        let var_idx = vars.add(var_name, false);
        let mut prog = ErlProgram::new("TestProg");
        let lit_idx = prog.add_literal(value);
        let expr_idx = prog.add_expression(ErlExpression {
            op: ErlOp::Literal(lit_idx),
            operands: vec![],
        });
        prog.instructions.push(ErlInstruction {
            keyword: ErlKeyword::Set,
            arg1: var_idx,
            arg2: expr_idx,
        });
        prog
    }

    #[test]
    fn simple_set_program() {
        let mut vars = VariableManager::default();
        let prog = build_set_program(&mut vars, "result", 42.0);
        prog.execute(&mut vars);
        let idx = vars.find("result").unwrap();
        assert!((vars.get(idx).unwrap().as_number() - 42.0).abs() < 1e-10);
    }

    #[test]
    fn arithmetic_expression() {
        let mut vars = VariableManager::default();
        let result_idx = vars.add("result", false);
        let mut prog = ErlProgram::new("Arith");

        // Build expression: 3.0 + 4.0 * 2.0
        // First: literal 3.0
        let lit3 = prog.add_literal(3.0);
        let expr_3 = prog.add_expression(ErlExpression { op: ErlOp::Literal(lit3), operands: vec![] });
        // literal 4.0
        let lit4 = prog.add_literal(4.0);
        let expr_4 = prog.add_expression(ErlExpression { op: ErlOp::Literal(lit4), operands: vec![] });
        // literal 2.0
        let lit2 = prog.add_literal(2.0);
        let expr_2 = prog.add_expression(ErlExpression { op: ErlOp::Literal(lit2), operands: vec![] });
        // 4.0 * 2.0
        let expr_mul = prog.add_expression(ErlExpression { op: ErlOp::Multiply, operands: vec![expr_4, expr_2] });
        // 3.0 + (4.0 * 2.0)
        let expr_add = prog.add_expression(ErlExpression { op: ErlOp::Add, operands: vec![expr_3, expr_mul] });

        prog.instructions.push(ErlInstruction {
            keyword: ErlKeyword::Set,
            arg1: result_idx,
            arg2: expr_add,
        });

        prog.execute(&mut vars);
        assert!((vars.get(result_idx).unwrap().as_number() - 11.0).abs() < 1e-10);
    }

    #[test]
    fn comparison_operators() {
        let mut vars = VariableManager::default();
        let result_idx = vars.add("result", false);
        let mut prog = ErlProgram::new("Compare");

        let lit5 = prog.add_literal(5.0);
        let lit3 = prog.add_literal(3.0);
        let expr_5 = prog.add_expression(ErlExpression { op: ErlOp::Literal(lit5), operands: vec![] });
        let expr_3 = prog.add_expression(ErlExpression { op: ErlOp::Literal(lit3), operands: vec![] });
        let expr_gt = prog.add_expression(ErlExpression { op: ErlOp::GreaterThan, operands: vec![expr_5, expr_3] });

        prog.instructions.push(ErlInstruction {
            keyword: ErlKeyword::Set,
            arg1: result_idx,
            arg2: expr_gt,
        });

        prog.execute(&mut vars);
        assert!(vars.get(result_idx).unwrap().is_true()); // 5 > 3 = true
    }

    #[test]
    fn if_else_program() {
        let mut vars = VariableManager::default();
        vars.register_builtins();
        let x_idx = vars.add("x", false);
        let result_idx = vars.add("result", false);
        vars.set(x_idx, ErlValue::Number(10.0));

        let mut prog = ErlProgram::new("IfElse");

        // Expression: x > 5
        let var_x = prog.add_expression(ErlExpression { op: ErlOp::Variable(x_idx), operands: vec![] });
        let lit5 = prog.add_literal(5.0);
        let expr_5 = prog.add_expression(ErlExpression { op: ErlOp::Literal(lit5), operands: vec![] });
        let cond = prog.add_expression(ErlExpression { op: ErlOp::GreaterThan, operands: vec![var_x, expr_5] });

        // Literal expressions for results
        let lit100 = prog.add_literal(100.0);
        let expr_100 = prog.add_expression(ErlExpression { op: ErlOp::Literal(lit100), operands: vec![] });
        let lit0 = prog.add_literal(0.0);
        let expr_0 = prog.add_expression(ErlExpression { op: ErlOp::Literal(lit0), operands: vec![] });

        // IF x > 5, jump to else (inst 3) if false
        prog.instructions.push(ErlInstruction { keyword: ErlKeyword::If, arg1: 3, arg2: cond });
        // SET result = 100 (if-true body)
        prog.instructions.push(ErlInstruction { keyword: ErlKeyword::Set, arg1: result_idx, arg2: expr_100 });
        // GOTO endif (skip else)
        prog.instructions.push(ErlInstruction { keyword: ErlKeyword::Goto, arg1: 5, arg2: 0 });
        // ELSE
        prog.instructions.push(ErlInstruction { keyword: ErlKeyword::Else, arg1: 0, arg2: 0 });
        // SET result = 0 (else body)
        prog.instructions.push(ErlInstruction { keyword: ErlKeyword::Set, arg1: result_idx, arg2: expr_0 });
        // ENDIF
        prog.instructions.push(ErlInstruction { keyword: ErlKeyword::EndIf, arg1: 0, arg2: 0 });

        prog.execute(&mut vars);
        assert!((vars.get(result_idx).unwrap().as_number() - 100.0).abs() < 1e-10);
    }

    #[test]
    fn while_loop_program() {
        let mut vars = VariableManager::default();
        let counter_idx = vars.add("counter", false);
        let sum_idx = vars.add("sum", false);
        vars.set(counter_idx, ErlValue::Number(0.0));
        vars.set(sum_idx, ErlValue::Number(0.0));

        let mut prog = ErlProgram::new("WhileLoop");

        // Expressions
        let var_counter = prog.add_expression(ErlExpression { op: ErlOp::Variable(counter_idx), operands: vec![] });
        let _var_sum = prog.add_expression(ErlExpression { op: ErlOp::Variable(sum_idx), operands: vec![] });
        let lit5 = prog.add_literal(5.0);
        let expr_5 = prog.add_expression(ErlExpression { op: ErlOp::Literal(lit5), operands: vec![] });
        let lit1 = prog.add_literal(1.0);
        let expr_1 = prog.add_expression(ErlExpression { op: ErlOp::Literal(lit1), operands: vec![] });

        // counter < 5
        let cond = prog.add_expression(ErlExpression { op: ErlOp::LessThan, operands: vec![var_counter, expr_5] });

        // sum = sum + counter (need fresh variable refs)
        let var_sum2 = prog.add_expression(ErlExpression { op: ErlOp::Variable(sum_idx), operands: vec![] });
        let var_counter2 = prog.add_expression(ErlExpression { op: ErlOp::Variable(counter_idx), operands: vec![] });
        let sum_plus_counter = prog.add_expression(ErlExpression { op: ErlOp::Add, operands: vec![var_sum2, var_counter2] });

        // counter = counter + 1
        let var_counter3 = prog.add_expression(ErlExpression { op: ErlOp::Variable(counter_idx), operands: vec![] });
        let counter_plus_1 = prog.add_expression(ErlExpression { op: ErlOp::Add, operands: vec![var_counter3, expr_1] });

        // WHILE counter < 5 (jump to inst 4 if false)
        prog.instructions.push(ErlInstruction { keyword: ErlKeyword::While, arg1: 4, arg2: cond });
        // SET sum = sum + counter
        prog.instructions.push(ErlInstruction { keyword: ErlKeyword::Set, arg1: sum_idx, arg2: sum_plus_counter });
        // SET counter = counter + 1
        prog.instructions.push(ErlInstruction { keyword: ErlKeyword::Set, arg1: counter_idx, arg2: counter_plus_1 });
        // ENDWHILE (jump back to inst 0)
        prog.instructions.push(ErlInstruction { keyword: ErlKeyword::EndWhile, arg1: 0, arg2: 0 });

        prog.execute(&mut vars);
        // sum = 0 + 1 + 2 + 3 + 4 = 10
        assert!((vars.get(sum_idx).unwrap().as_number() - 10.0).abs() < 1e-10);
        assert!((vars.get(counter_idx).unwrap().as_number() - 5.0).abs() < 1e-10);
    }

    #[test]
    fn math_functions() {
        let mut vars = VariableManager::default();
        let result_idx = vars.add("result", false);
        let mut prog = ErlProgram::new("MathFunc");

        // @SIN(PI/2) = 1.0
        let lit_pi2 = prog.add_literal(std::f64::consts::FRAC_PI_2);
        let expr_pi2 = prog.add_expression(ErlExpression { op: ErlOp::Literal(lit_pi2), operands: vec![] });
        let expr_sin = prog.add_expression(ErlExpression { op: ErlOp::Sin, operands: vec![expr_pi2] });

        prog.instructions.push(ErlInstruction {
            keyword: ErlKeyword::Set, arg1: result_idx, arg2: expr_sin,
        });

        prog.execute(&mut vars);
        assert!((vars.get(result_idx).unwrap().as_number() - 1.0).abs() < 1e-10);
    }

    #[test]
    fn psychrometric_function() {
        let mut vars = VariableManager::default();
        let result_idx = vars.add("cp", false);
        let mut prog = ErlProgram::new("PsychFunc");

        // @CPAIRFNW(0.008) should return ~1012 J/kg-K
        let lit_w = prog.add_literal(0.008);
        let expr_w = prog.add_expression(ErlExpression { op: ErlOp::Literal(lit_w), operands: vec![] });
        let expr_cp = prog.add_expression(ErlExpression { op: ErlOp::CpAir, operands: vec![expr_w] });

        prog.instructions.push(ErlInstruction {
            keyword: ErlKeyword::Set, arg1: result_idx, arg2: expr_cp,
        });

        prog.execute(&mut vars);
        let cp = vars.get(result_idx).unwrap().as_number();
        assert!(cp > 1000.0 && cp < 1100.0, "cp_air={}", cp);
    }

    #[test]
    fn divide_by_zero_error() {
        let mut vars = VariableManager::default();
        let result_idx = vars.add("result", false);
        let mut prog = ErlProgram::new("DivZero");

        let lit1 = prog.add_literal(1.0);
        let lit0 = prog.add_literal(0.0);
        let expr_1 = prog.add_expression(ErlExpression { op: ErlOp::Literal(lit1), operands: vec![] });
        let expr_0 = prog.add_expression(ErlExpression { op: ErlOp::Literal(lit0), operands: vec![] });
        let expr_div = prog.add_expression(ErlExpression { op: ErlOp::Divide, operands: vec![expr_1, expr_0] });

        prog.instructions.push(ErlInstruction {
            keyword: ErlKeyword::Set, arg1: result_idx, arg2: expr_div,
        });

        prog.execute(&mut vars);
        // Result should be error, but set still happens (error value)
        let val = vars.get(result_idx).unwrap();
        assert!(val.is_error());
    }

    #[test]
    fn return_early() {
        let mut vars = VariableManager::default();
        let a_idx = vars.add("a", false);
        let b_idx = vars.add("b", false);
        let mut prog = ErlProgram::new("ReturnEarly");

        let lit1 = prog.add_literal(1.0);
        let expr_1 = prog.add_expression(ErlExpression { op: ErlOp::Literal(lit1), operands: vec![] });
        let lit2 = prog.add_literal(2.0);
        let expr_2 = prog.add_expression(ErlExpression { op: ErlOp::Literal(lit2), operands: vec![] });

        prog.instructions.push(ErlInstruction { keyword: ErlKeyword::Set, arg1: a_idx, arg2: expr_1 });
        prog.instructions.push(ErlInstruction { keyword: ErlKeyword::Return, arg1: 0, arg2: 0 });
        prog.instructions.push(ErlInstruction { keyword: ErlKeyword::Set, arg1: b_idx, arg2: expr_2 });

        prog.execute(&mut vars);
        assert!((vars.get(a_idx).unwrap().as_number() - 1.0).abs() < 1e-10);
        assert!(vars.get(b_idx).unwrap().is_null()); // Never reached
    }

    #[test]
    fn elseif_chain() {
        // IF x > 10 → SET result = 1 (false)
        // ELSEIF x > 5 → SET result = 2 (true, x=7)
        // ELSE → SET result = 3
        // ENDIF
        let mut vars = VariableManager::default();
        let x_idx = vars.add("x", false);
        let result_idx = vars.add("result", false);
        vars.set(x_idx, ErlValue::Number(7.0));

        let mut prog = ErlProgram::new("ElseIfChain");

        // Expressions
        let var_x = prog.add_expression(ErlExpression { op: ErlOp::Variable(x_idx), operands: vec![] });
        let lit10 = prog.add_literal(10.0);
        let expr_10 = prog.add_expression(ErlExpression { op: ErlOp::Literal(lit10), operands: vec![] });
        let lit5 = prog.add_literal(5.0);
        let expr_5 = prog.add_expression(ErlExpression { op: ErlOp::Literal(lit5), operands: vec![] });
        let cond_gt10 = prog.add_expression(ErlExpression { op: ErlOp::GreaterThan, operands: vec![var_x, expr_10] });
        let var_x2 = prog.add_expression(ErlExpression { op: ErlOp::Variable(x_idx), operands: vec![] });
        let cond_gt5 = prog.add_expression(ErlExpression { op: ErlOp::GreaterThan, operands: vec![var_x2, expr_5] });

        let lit1 = prog.add_literal(1.0);
        let expr_1 = prog.add_expression(ErlExpression { op: ErlOp::Literal(lit1), operands: vec![] });
        let lit2 = prog.add_literal(2.0);
        let expr_2 = prog.add_expression(ErlExpression { op: ErlOp::Literal(lit2), operands: vec![] });
        let lit3 = prog.add_literal(3.0);
        let expr_3 = prog.add_expression(ErlExpression { op: ErlOp::Literal(lit3), operands: vec![] });

        // inst 0: IF x > 10, false → jump to inst 3 (ELSEIF)
        prog.instructions.push(ErlInstruction { keyword: ErlKeyword::If, arg1: 3, arg2: cond_gt10 });
        // inst 1: SET result = 1
        prog.instructions.push(ErlInstruction { keyword: ErlKeyword::Set, arg1: result_idx, arg2: expr_1 });
        // inst 2: GOTO endif (inst 8)
        prog.instructions.push(ErlInstruction { keyword: ErlKeyword::Goto, arg1: 8, arg2: 0 });
        // inst 3: ELSEIF x > 5, false → jump to inst 6 (ELSE)
        prog.instructions.push(ErlInstruction { keyword: ErlKeyword::ElseIf, arg1: 6, arg2: cond_gt5 });
        // inst 4: SET result = 2
        prog.instructions.push(ErlInstruction { keyword: ErlKeyword::Set, arg1: result_idx, arg2: expr_2 });
        // inst 5: GOTO endif (inst 8)
        prog.instructions.push(ErlInstruction { keyword: ErlKeyword::Goto, arg1: 8, arg2: 0 });
        // inst 6: ELSE
        prog.instructions.push(ErlInstruction { keyword: ErlKeyword::Else, arg1: 0, arg2: 0 });
        // inst 7: SET result = 3
        prog.instructions.push(ErlInstruction { keyword: ErlKeyword::Set, arg1: result_idx, arg2: expr_3 });
        // inst 8: ENDIF
        prog.instructions.push(ErlInstruction { keyword: ErlKeyword::EndIf, arg1: 0, arg2: 0 });

        prog.execute(&mut vars);
        assert!((vars.get(result_idx).unwrap().as_number() - 2.0).abs() < 1e-10);
    }

    #[test]
    fn nested_if() {
        // IF x > 0 → IF y > 0 → SET result = 1
        let mut vars = VariableManager::default();
        let x_idx = vars.add("x", false);
        let y_idx = vars.add("y", false);
        let result_idx = vars.add("result", false);
        vars.set(x_idx, ErlValue::Number(5.0));
        vars.set(y_idx, ErlValue::Number(3.0));

        let mut prog = ErlProgram::new("NestedIf");

        let var_x = prog.add_expression(ErlExpression { op: ErlOp::Variable(x_idx), operands: vec![] });
        let var_y = prog.add_expression(ErlExpression { op: ErlOp::Variable(y_idx), operands: vec![] });
        let lit0 = prog.add_literal(0.0);
        let expr_0 = prog.add_expression(ErlExpression { op: ErlOp::Literal(lit0), operands: vec![] });
        let expr_0b = prog.add_expression(ErlExpression { op: ErlOp::Literal(lit0), operands: vec![] });
        let cond_x = prog.add_expression(ErlExpression { op: ErlOp::GreaterThan, operands: vec![var_x, expr_0] });
        let cond_y = prog.add_expression(ErlExpression { op: ErlOp::GreaterThan, operands: vec![var_y, expr_0b] });

        let lit1 = prog.add_literal(1.0);
        let expr_1 = prog.add_expression(ErlExpression { op: ErlOp::Literal(lit1), operands: vec![] });

        // inst 0: IF x > 0, false → jump to inst 4 (outer endif)
        prog.instructions.push(ErlInstruction { keyword: ErlKeyword::If, arg1: 4, arg2: cond_x });
        // inst 1: IF y > 0, false → jump to inst 3 (inner endif)
        prog.instructions.push(ErlInstruction { keyword: ErlKeyword::If, arg1: 3, arg2: cond_y });
        // inst 2: SET result = 1
        prog.instructions.push(ErlInstruction { keyword: ErlKeyword::Set, arg1: result_idx, arg2: expr_1 });
        // inst 3: ENDIF (inner)
        prog.instructions.push(ErlInstruction { keyword: ErlKeyword::EndIf, arg1: 0, arg2: 0 });
        // inst 4: ENDIF (outer)
        prog.instructions.push(ErlInstruction { keyword: ErlKeyword::EndIf, arg1: 0, arg2: 0 });

        prog.execute(&mut vars);
        assert!((vars.get(result_idx).unwrap().as_number() - 1.0).abs() < 1e-10);

        // Now test with y <= 0: result should stay at Null
        let mut vars2 = VariableManager::default();
        let x2_idx = vars2.add("x", false);
        let y2_idx = vars2.add("y", false);
        let result2_idx = vars2.add("result", false);
        vars2.set(x2_idx, ErlValue::Number(5.0));
        vars2.set(y2_idx, ErlValue::Number(-1.0));

        prog.execute(&mut vars2);
        assert!(vars2.get(result2_idx).unwrap().is_null());
    }

    #[test]
    fn while_zero_iterations() {
        // WHILE false → body never executes
        let mut vars = VariableManager::default();
        let result_idx = vars.add("result", false);
        vars.set(result_idx, ErlValue::Number(0.0));

        let mut prog = ErlProgram::new("WhileZero");

        // Condition: 0 < 0 (always false)
        let lit0a = prog.add_literal(0.0);
        let expr_0a = prog.add_expression(ErlExpression { op: ErlOp::Literal(lit0a), operands: vec![] });
        let lit0b = prog.add_literal(0.0);
        let expr_0b = prog.add_expression(ErlExpression { op: ErlOp::Literal(lit0b), operands: vec![] });
        let cond = prog.add_expression(ErlExpression { op: ErlOp::LessThan, operands: vec![expr_0a, expr_0b] });

        let lit99 = prog.add_literal(99.0);
        let expr_99 = prog.add_expression(ErlExpression { op: ErlOp::Literal(lit99), operands: vec![] });

        // inst 0: WHILE false → jump to inst 3
        prog.instructions.push(ErlInstruction { keyword: ErlKeyword::While, arg1: 3, arg2: cond });
        // inst 1: SET result = 99 (should NOT execute)
        prog.instructions.push(ErlInstruction { keyword: ErlKeyword::Set, arg1: result_idx, arg2: expr_99 });
        // inst 2: ENDWHILE → jump to inst 0
        prog.instructions.push(ErlInstruction { keyword: ErlKeyword::EndWhile, arg1: 0, arg2: 0 });

        prog.execute(&mut vars);
        // result should remain 0.0
        assert!((vars.get(result_idx).unwrap().as_number()).abs() < 1e-10);
    }

    #[test]
    fn while_max_iterations_safety() {
        // WHILE true → infinite loop, should terminate at max iterations
        let mut vars = VariableManager::default();
        let counter_idx = vars.add("counter", false);
        vars.set(counter_idx, ErlValue::Number(0.0));

        let mut prog = ErlProgram::new("InfiniteWhile");

        // Condition: 1 == 1 (always true)
        let lit1a = prog.add_literal(1.0);
        let expr_1a = prog.add_expression(ErlExpression { op: ErlOp::Literal(lit1a), operands: vec![] });
        let lit1b = prog.add_literal(1.0);
        let expr_1b = prog.add_expression(ErlExpression { op: ErlOp::Literal(lit1b), operands: vec![] });
        let cond = prog.add_expression(ErlExpression { op: ErlOp::Equal, operands: vec![expr_1a, expr_1b] });

        // Body: counter = counter + 1
        let var_ctr = prog.add_expression(ErlExpression { op: ErlOp::Variable(counter_idx), operands: vec![] });
        let lit1 = prog.add_literal(1.0);
        let expr_1 = prog.add_expression(ErlExpression { op: ErlOp::Literal(lit1), operands: vec![] });
        let add_expr = prog.add_expression(ErlExpression { op: ErlOp::Add, operands: vec![var_ctr, expr_1] });

        // inst 0: WHILE true → jump to inst 3 if false (never)
        prog.instructions.push(ErlInstruction { keyword: ErlKeyword::While, arg1: 3, arg2: cond });
        // inst 1: SET counter = counter + 1
        prog.instructions.push(ErlInstruction { keyword: ErlKeyword::Set, arg1: counter_idx, arg2: add_expr });
        // inst 2: ENDWHILE → jump to inst 0
        prog.instructions.push(ErlInstruction { keyword: ErlKeyword::EndWhile, arg1: 0, arg2: 0 });

        prog.execute(&mut vars);
        // Should have stopped due to max_iterations (1_000_000) safety
        let count = vars.get(counter_idx).unwrap().as_number();
        assert!(count > 0.0, "counter should have incremented");
        assert!(count <= 1_000_001.0, "counter should be bounded by max iterations");
    }

    #[test]
    fn goto_instruction() {
        // SET a = 1
        // GOTO inst 3 (skip SET b = 2)
        // SET b = 2
        // (end)
        let mut vars = VariableManager::default();
        let a_idx = vars.add("a", false);
        let b_idx = vars.add("b", false);

        let mut prog = ErlProgram::new("GotoProg");

        let lit1 = prog.add_literal(1.0);
        let expr_1 = prog.add_expression(ErlExpression { op: ErlOp::Literal(lit1), operands: vec![] });
        let lit2 = prog.add_literal(2.0);
        let expr_2 = prog.add_expression(ErlExpression { op: ErlOp::Literal(lit2), operands: vec![] });

        // inst 0: SET a = 1
        prog.instructions.push(ErlInstruction { keyword: ErlKeyword::Set, arg1: a_idx, arg2: expr_1 });
        // inst 1: GOTO inst 3 (past end of instructions, exit)
        prog.instructions.push(ErlInstruction { keyword: ErlKeyword::Goto, arg1: 3, arg2: 0 });
        // inst 2: SET b = 2 (should be skipped)
        prog.instructions.push(ErlInstruction { keyword: ErlKeyword::Set, arg1: b_idx, arg2: expr_2 });

        prog.execute(&mut vars);
        assert!((vars.get(a_idx).unwrap().as_number() - 1.0).abs() < 1e-10);
        assert!(vars.get(b_idx).unwrap().is_null()); // Skipped by goto
    }

    #[test]
    fn negate_operation() {
        // SET result = -5.0 using Negate op
        let mut vars = VariableManager::default();
        let result_idx = vars.add("result", false);

        let mut prog = ErlProgram::new("NegateProg");

        let lit5 = prog.add_literal(5.0);
        let expr_5 = prog.add_expression(ErlExpression { op: ErlOp::Literal(lit5), operands: vec![] });
        let expr_neg = prog.add_expression(ErlExpression { op: ErlOp::Negate, operands: vec![expr_5] });

        prog.instructions.push(ErlInstruction {
            keyword: ErlKeyword::Set,
            arg1: result_idx,
            arg2: expr_neg,
        });

        prog.execute(&mut vars);
        assert!((vars.get(result_idx).unwrap().as_number() - (-5.0)).abs() < 1e-10);
    }

    #[test]
    fn power_operation() {
        // SET result = 2^10 = 1024
        let mut vars = VariableManager::default();
        let result_idx = vars.add("result", false);

        let mut prog = ErlProgram::new("PowerProg");

        let lit2 = prog.add_literal(2.0);
        let expr_2 = prog.add_expression(ErlExpression { op: ErlOp::Literal(lit2), operands: vec![] });
        let lit10 = prog.add_literal(10.0);
        let expr_10 = prog.add_expression(ErlExpression { op: ErlOp::Literal(lit10), operands: vec![] });
        let expr_pow = prog.add_expression(ErlExpression { op: ErlOp::Power, operands: vec![expr_2, expr_10] });

        prog.instructions.push(ErlInstruction {
            keyword: ErlKeyword::Set,
            arg1: result_idx,
            arg2: expr_pow,
        });

        prog.execute(&mut vars);
        assert!((vars.get(result_idx).unwrap().as_number() - 1024.0).abs() < 1e-10);
    }

    #[test]
    fn logical_and_or() {
        // AND(1, 0) = 0, OR(1, 0) = 1
        let mut vars = VariableManager::default();
        let and_idx = vars.add("and_result", false);
        let or_idx = vars.add("or_result", false);

        let mut prog = ErlProgram::new("LogicProg");

        let lit1 = prog.add_literal(1.0);
        let expr_1a = prog.add_expression(ErlExpression { op: ErlOp::Literal(lit1), operands: vec![] });
        let lit0 = prog.add_literal(0.0);
        let expr_0a = prog.add_expression(ErlExpression { op: ErlOp::Literal(lit0), operands: vec![] });
        let expr_and = prog.add_expression(ErlExpression { op: ErlOp::And, operands: vec![expr_1a, expr_0a] });

        let expr_1b = prog.add_expression(ErlExpression { op: ErlOp::Literal(lit1), operands: vec![] });
        let expr_0b = prog.add_expression(ErlExpression { op: ErlOp::Literal(lit0), operands: vec![] });
        let expr_or = prog.add_expression(ErlExpression { op: ErlOp::Or, operands: vec![expr_1b, expr_0b] });

        // SET and_result = AND(1, 0)
        prog.instructions.push(ErlInstruction {
            keyword: ErlKeyword::Set,
            arg1: and_idx,
            arg2: expr_and,
        });
        // SET or_result = OR(1, 0)
        prog.instructions.push(ErlInstruction {
            keyword: ErlKeyword::Set,
            arg1: or_idx,
            arg2: expr_or,
        });

        prog.execute(&mut vars);
        assert!((vars.get(and_idx).unwrap().as_number() - 0.0).abs() < 1e-10);
        assert!((vars.get(or_idx).unwrap().as_number() - 1.0).abs() < 1e-10);
    }
}
