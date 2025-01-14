use acir::{
    circuit::{Circuit, Opcode},
    native_types::Witness,
    BlackBoxFunc,
};
use std::collections::{BTreeMap, HashSet};

/// RangeOptimizer will remove redundant range constraints.
///
/// Suppose we had the following pseudo-code:
///
/// let z1 = x as u16;
//  let z2 = x as u32;
///
/// It is clear that if `x` fits inside of a 16-bit integer,
/// it must also fit inside of a 32-bit integer.
///
/// The generated ACIR may produce two range opcodes however;
/// - One for the 16 bit range constraint of `x`
/// - One for the 32-bit range constraint of `x`
///
/// This optimization pass will keep the 16-bit range constraint
/// and remove the 32-bit range constraint opcode.
pub(crate) struct RangeOptimizer {
    /// Maps witnesses to their lowest known bit sizes.
    lists: BTreeMap<Witness, u32>,
    circuit: Circuit,
}

impl RangeOptimizer {
    /// Creates a new `RangeOptimizer` by collecting all known range
    /// constraints from `Circuit`.
    pub(crate) fn new(circuit: Circuit) -> Self {
        let range_list = Self::collect_ranges(&circuit);
        Self { circuit, lists: range_list }
    }

    /// Stores the lowest bit range, that a witness
    /// has been constrained to be.
    /// For example, if we constrain a witness `x` to be
    /// both 32 bits and 16 bits. This function will
    /// only store the fact that we have constrained it to
    /// be 16 bits.
    fn collect_ranges(circuit: &Circuit) -> BTreeMap<Witness, u32> {
        let mut witness_to_bit_sizes = BTreeMap::new();

        for opcode in &circuit.opcodes {
            // Extract the witness index and number of bits,
            // if it is a range constraint
            let (witness, num_bits) = match extract_range_opcode(opcode) {
                Some(func_inputs) => func_inputs,
                None => continue,
            };

            // Check if the witness has already been recorded and if the witness
            // size is more than the current one, we replace it
            let should_replace = match witness_to_bit_sizes.get(&witness).copied() {
                Some(old_range_bits) => old_range_bits > num_bits,
                None => true,
            };
            if should_replace {
                witness_to_bit_sizes.insert(witness, num_bits);
            }
        }
        witness_to_bit_sizes
    }

    /// Returns a `Circuit` where each Witness is only range constrained
    /// once to the lowest number `bit size` possible.
    pub(crate) fn replace_redundant_ranges(self) -> Circuit {
        let mut already_seen_witness = HashSet::new();

        let mut optimized_opcodes = Vec::with_capacity(self.circuit.opcodes.len());

        for opcode in self.circuit.opcodes {
            let (witness, num_bits) = match extract_range_opcode(&opcode) {
                Some(range_opcode) => range_opcode,
                None => {
                    // If its not the range opcode, add it to the opcode
                    // list and continue;
                    optimized_opcodes.push(opcode);
                    continue;
                }
            };
            // If we've already applied the range constraint for this witness then skip this opcode.
            let already_added = already_seen_witness.contains(&witness);
            if already_added {
                continue;
            }

            // Check if this is the lowest number of bits in the circuit
            let stored_num_bits = self.lists.get(&witness).expect("Could not find witness. This should never be the case if `collect_ranges` is called");
            let is_lowest_bit_size = num_bits <= *stored_num_bits;

            // If the opcode is associated with the lowest bit size
            // and we have not added a duplicate of this opcode yet,
            // then we should add retain this range opcode.
            if is_lowest_bit_size {
                already_seen_witness.insert(witness);
                optimized_opcodes.push(opcode);
            }
        }

        Circuit {
            current_witness_index: self.circuit.current_witness_index,
            opcodes: optimized_opcodes,
            public_parameters: self.circuit.public_parameters,
            return_values: self.circuit.return_values,
        }
    }
}

/// Extract the range opcode from the `Opcode` enum
/// Returns None, if `Opcode` is not the range opcode.
fn extract_range_opcode(opcode: &Opcode) -> Option<(Witness, u32)> {
    // Range constraints are blackbox function calls
    // so we first extract the function call
    let func_call = match opcode {
        acir::circuit::Opcode::BlackBoxFuncCall(func_call) => func_call,
        _ => return None,
    };

    // Skip if it is not a range constraint
    if func_call.name != BlackBoxFunc::RANGE {
        return None;
    }

    let func_input =
        func_call.inputs.first().expect("we expect there to be one input for a range call");
    Some((func_input.witness, func_input.num_bits))
}

#[cfg(test)]
mod tests {
    use crate::compiler::optimizers::redundant_range::{extract_range_opcode, RangeOptimizer};
    use acir::{
        circuit::{
            opcodes::{BlackBoxFuncCall, FunctionInput},
            Circuit, Opcode, PublicInputs,
        },
        native_types::{Expression, Witness},
        BlackBoxFunc,
    };

    fn test_circuit(ranges: Vec<(Witness, u32)>) -> Circuit {
        fn test_range_constraint(witness: Witness, num_bits: u32) -> Opcode {
            Opcode::BlackBoxFuncCall(BlackBoxFuncCall {
                name: BlackBoxFunc::RANGE,
                inputs: vec![FunctionInput { witness, num_bits }],
                outputs: vec![],
            })
        }

        let opcodes: Vec<_> = ranges
            .into_iter()
            .map(|(witness, num_bits)| test_range_constraint(witness, num_bits))
            .collect();

        Circuit {
            current_witness_index: 1,
            opcodes,
            public_parameters: PublicInputs::default(),
            return_values: PublicInputs::default(),
        }
    }

    #[test]
    fn retain_lowest_range_size() {
        // The optimizer should keep the lowest bit size range constraint
        let circuit = test_circuit(vec![(Witness(1), 32), (Witness(1), 16)]);

        let optimizer = RangeOptimizer::new(circuit);

        let range_size = *optimizer
            .lists
            .get(&Witness(1))
            .expect("Witness(1) was inserted, but it is missing from the map");
        assert_eq!(
            range_size, 16,
            "expected a range size of 16 since that was the lowest bit size provided"
        );

        let optimized_circuit = optimizer.replace_redundant_ranges();
        assert_eq!(optimized_circuit.opcodes.len(), 1);

        let (witness, num_bits) =
            extract_range_opcode(&optimized_circuit.opcodes[0]).expect("expected one range opcode");

        assert_eq!(witness, Witness(1));
        assert_eq!(num_bits, 16);
    }

    #[test]
    fn remove_duplicates() {
        // The optimizer should remove all duplicate range opcodes.

        let circuit = test_circuit(vec![
            (Witness(1), 16),
            (Witness(1), 16),
            (Witness(2), 23),
            (Witness(2), 23),
        ]);

        let optimizer = RangeOptimizer::new(circuit);
        let optimized_circuit = optimizer.replace_redundant_ranges();
        assert_eq!(optimized_circuit.opcodes.len(), 2);

        let (witness_a, num_bits_a) =
            extract_range_opcode(&optimized_circuit.opcodes[0]).expect("expected two range opcode");
        let (witness_b, num_bits_b) =
            extract_range_opcode(&optimized_circuit.opcodes[1]).expect("expected two range opcode");

        assert_eq!(witness_a, Witness(1));
        assert_eq!(witness_b, Witness(2));
        assert_eq!(num_bits_a, 16);
        assert_eq!(num_bits_b, 23);
    }

    #[test]
    fn non_range_opcodes() {
        // The optimizer should not remove or change non-range opcodes
        // The four Arithmetic opcodes should remain unchanged.
        let mut circuit = test_circuit(vec![(Witness(1), 16), (Witness(1), 16)]);

        circuit.opcodes.push(Opcode::Arithmetic(Expression::default()));
        circuit.opcodes.push(Opcode::Arithmetic(Expression::default()));
        circuit.opcodes.push(Opcode::Arithmetic(Expression::default()));
        circuit.opcodes.push(Opcode::Arithmetic(Expression::default()));

        let optimizer = RangeOptimizer::new(circuit);
        let optimized_circuit = optimizer.replace_redundant_ranges();
        assert_eq!(optimized_circuit.opcodes.len(), 5)
    }
}
