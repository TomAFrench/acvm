#![warn(unused_crate_dependencies)]
#![warn(unreachable_pub)]

// Key is currently {NPComplete_lang}_{OptionalFanIn}_ProofSystem_OrgName
// Org name is needed because more than one implementation of the same proof system may arise

pub mod compiler;
pub mod pwg;

use acir::{
    circuit::{
        opcodes::{BlackBoxFuncCall, FunctionInput},
        Circuit, Opcode,
    },
    native_types::{Expression, Witness},
    BlackBoxFunc,
};
use std::collections::BTreeMap;
use thiserror::Error;

// re-export acir
pub use acir;
pub use acir::FieldElement;

// This enum represents the different cases in which an
// opcode can be unsolvable.
// The most common being that one of its input has not been
// assigned a value.
//
// TODO: ExpressionHasTooManyUnknowns is specific for arithmetic expressions
// TODO: we could have a error enum for arithmetic failure cases in that module
// TODO that can be converted into an OpcodeNotSolvable or OpcodeResolutionError enum
#[derive(PartialEq, Eq, Debug, Error)]
pub enum OpcodeNotSolvable {
    #[error("missing assignment for witness index {0}")]
    MissingAssignment(u32),
    #[error("expression has too many unknowns {0}")]
    ExpressionHasTooManyUnknowns(Expression),
}

#[derive(PartialEq, Eq, Debug, Error)]
pub enum OpcodeResolutionError {
    #[error("cannot solve opcode: {0}")]
    OpcodeNotSolvable(#[from] OpcodeNotSolvable),
    #[error("backend does not currently support the {0} opcode. ACVM does not currently have a fallback for this opcode.")]
    UnsupportedBlackBoxFunc(BlackBoxFunc),
    #[error("could not satisfy all constraints")]
    UnsatisfiedConstrain,
    #[error("unexpected opcode, expected {0}, but got {1}")]
    UnexpectedOpcode(&'static str, BlackBoxFunc),
    #[error("expected {0} inputs for function {1}, but got {2}")]
    IncorrectNumFunctionArguments(usize, BlackBoxFunc, usize),
    #[error("failed to solve blackbox function: {0}, reason: {1}")]
    BlackBoxFunctionFailed(BlackBoxFunc, String),
}

pub trait Backend: SmartContract + ProofSystemCompiler + PartialWitnessGenerator + Default {}

/// This component will generate the backend specific output for
/// each OPCODE.
/// Returns an Error if the backend does not support that OPCODE
pub trait PartialWitnessGenerator {
    fn aes(
        &self,
        initial_witness: &mut BTreeMap<Witness, FieldElement>,
        inputs: &[FunctionInput],
        outputs: &[Witness],
    ) -> Result<pwg::OpcodeResolution, OpcodeResolutionError>;
    fn and(
        &self,
        initial_witness: &mut BTreeMap<Witness, FieldElement>,
        inputs: &[FunctionInput],
        outputs: &[Witness],
    ) -> Result<pwg::OpcodeResolution, OpcodeResolutionError>;
    fn xor(
        &self,
        initial_witness: &mut BTreeMap<Witness, FieldElement>,
        inputs: &[FunctionInput],
        outputs: &[Witness],
    ) -> Result<pwg::OpcodeResolution, OpcodeResolutionError>;
    fn range(
        &self,
        initial_witness: &mut BTreeMap<Witness, FieldElement>,
        inputs: &[FunctionInput],
        outputs: &[Witness],
    ) -> Result<pwg::OpcodeResolution, OpcodeResolutionError>;
    fn sha256(
        &self,
        initial_witness: &mut BTreeMap<Witness, FieldElement>,
        inputs: &[FunctionInput],
        outputs: &[Witness],
    ) -> Result<pwg::OpcodeResolution, OpcodeResolutionError>;
    fn blake2s(
        &self,
        initial_witness: &mut BTreeMap<Witness, FieldElement>,
        inputs: &[FunctionInput],
        outputs: &[Witness],
    ) -> Result<pwg::OpcodeResolution, OpcodeResolutionError>;
    fn compute_merkle_root(
        &self,
        initial_witness: &mut BTreeMap<Witness, FieldElement>,
        inputs: &[FunctionInput],
        outputs: &[Witness],
    ) -> Result<pwg::OpcodeResolution, OpcodeResolutionError>;
    fn schnorr_verify(
        &self,
        initial_witness: &mut BTreeMap<Witness, FieldElement>,
        inputs: &[FunctionInput],
        outputs: &[Witness],
    ) -> Result<pwg::OpcodeResolution, OpcodeResolutionError>;
    fn pedersen(
        &self,
        initial_witness: &mut BTreeMap<Witness, FieldElement>,
        inputs: &[FunctionInput],
        outputs: &[Witness],
    ) -> Result<pwg::OpcodeResolution, OpcodeResolutionError>;
    fn hash_to_field128_security(
        &self,
        initial_witness: &mut BTreeMap<Witness, FieldElement>,
        inputs: &[FunctionInput],
        outputs: &[Witness],
    ) -> Result<pwg::OpcodeResolution, OpcodeResolutionError>;
    fn ecdsa_secp256k1(
        &self,
        initial_witness: &mut BTreeMap<Witness, FieldElement>,
        inputs: &[FunctionInput],
        outputs: &[Witness],
    ) -> Result<pwg::OpcodeResolution, OpcodeResolutionError>;
    fn fixed_base_scalar_mul(
        &self,
        initial_witness: &mut BTreeMap<Witness, FieldElement>,
        inputs: &[FunctionInput],
        outputs: &[Witness],
    ) -> Result<pwg::OpcodeResolution, OpcodeResolutionError>;
    fn keccak256(
        &self,
        initial_witness: &mut BTreeMap<Witness, FieldElement>,
        inputs: &[FunctionInput],
        outputs: &[Witness],
    ) -> Result<pwg::OpcodeResolution, OpcodeResolutionError>;
}

pub trait SmartContract {
    /// The Error type returned by failed function calls in the SmartContract trait.
    type Error: std::error::Error; // fully-qualified named because thiserror is `use`d at the top of the crate

    // TODO: Allow a backend to support multiple smart contract platforms

    /// Returns an Ethereum smart contract to verify proofs against a given verification key.
    fn eth_contract_from_vk(&self, verification_key: &[u8]) -> Result<String, Self::Error>;
}

pub trait ProofSystemCompiler {
    /// The Error type returned by failed function calls in the ProofSystemCompiler trait.
    type Error: std::error::Error; // fully-qualified named because thiserror is `use`d at the top of the crate

    /// The NPC language that this proof system directly accepts.
    /// It is possible for ACVM to transpile to different languages, however it is advised to create a new backend
    /// as this in most cases will be inefficient. For this reason, we want to throw a hard error
    /// if the language and proof system does not line up.
    fn np_language(&self) -> Language;

    // Returns true if the backend supports the selected black box function
    fn black_box_function_supported(&self, opcode: &BlackBoxFunc) -> bool;

    /// Returns the number of gates in a circuit
    fn get_exact_circuit_size(&self, circuit: &Circuit) -> Result<u32, Self::Error>;

    /// Generates a proving and verification key given the circuit description
    /// These keys can then be used to construct a proof and for its verification
    fn preprocess(&self, circuit: &Circuit) -> Result<(Vec<u8>, Vec<u8>), Self::Error>;

    /// Creates a Proof given the circuit description, the initial witness values, and the proving key
    /// It is important to note that the intermediate witnesses for black box functions will not generated
    /// This is the responsibility of the proof system.
    fn prove_with_pk(
        &self,
        circuit: &Circuit,
        witness_values: BTreeMap<Witness, FieldElement>,
        proving_key: &[u8],
    ) -> Result<Vec<u8>, Self::Error>;

    /// Verifies a Proof, given the circuit description, the circuit's public inputs, and the verification key
    fn verify_with_vk(
        &self,
        proof: &[u8],
        public_inputs: BTreeMap<Witness, FieldElement>,
        circuit: &Circuit,
        verification_key: &[u8],
    ) -> Result<bool, Self::Error>;
}

/// Supported NP complete languages
/// This might need to be in ACIR instead
#[derive(Debug, Clone)]
pub enum Language {
    R1CS,
    PLONKCSat { width: usize },
}

#[deprecated]
pub fn hash_constraint_system(cs: &Circuit) -> [u8; 32] {
    let mut bytes = Vec::new();
    cs.write(&mut bytes).expect("could not serialize circuit");

    use sha2::{digest::FixedOutput, Digest, Sha256};
    let mut hasher = Sha256::new();

    hasher.update(bytes);
    hasher.finalize_fixed().into()
}

#[deprecated]
pub fn checksum_constraint_system(cs: &Circuit) -> u32 {
    let mut bytes = Vec::new();
    cs.write(&mut bytes).expect("could not serialize circuit");

    use crc32fast::Hasher;
    let mut hasher = Hasher::new();

    hasher.update(&bytes);
    hasher.finalize()
}

#[deprecated(
    note = "For backwards compatibility, this method allows you to derive _sensible_ defaults for black box function support based on the np language. \n Backends should simply specify what they support."
)]
// This is set to match the previous functionality that we had
// Where we could deduce what opcodes were supported
// by knowing the np complete language
pub fn default_is_opcode_supported(
    language: Language,
) -> compiler::transformers::IsOpcodeSupported {
    // R1CS does not support any of the opcode except Arithmetic by default.
    // The compiler will replace those that it can -- ie range, xor, and
    fn r1cs_is_supported(opcode: &Opcode) -> bool {
        matches!(opcode, Opcode::Arithmetic(_))
    }

    // PLONK supports most of the opcodes by default
    // The ones which are not supported, the acvm compiler will
    // attempt to transform into supported gates. If these are also not available
    // then a compiler error will be emitted.
    fn plonk_is_supported(opcode: &Opcode) -> bool {
        !matches!(
            opcode,
            Opcode::BlackBoxFuncCall(BlackBoxFuncCall { name: BlackBoxFunc::AES, .. })
                | Opcode::Block(_)
        )
    }

    match language {
        Language::R1CS => r1cs_is_supported,
        Language::PLONKCSat { .. } => plonk_is_supported,
    }
}

#[cfg(test)]
mod test {
    use std::collections::BTreeMap;

    use acir::{
        circuit::{
            directives::Directive,
            opcodes::{FunctionInput, OracleData},
            Opcode,
        },
        native_types::{Expression, Witness},
        FieldElement,
    };

    use crate::{
        pwg::{self, block::Blocks, OpcodeResolution, PartialWitnessGeneratorStatus},
        OpcodeResolutionError, PartialWitnessGenerator,
    };

    struct StubbedPwg;

    impl PartialWitnessGenerator for StubbedPwg {
        fn aes(
            &self,
            _initial_witness: &mut BTreeMap<Witness, FieldElement>,
            _inputs: &[FunctionInput],
            _outputs: &[Witness],
        ) -> Result<OpcodeResolution, OpcodeResolutionError> {
            {
                panic!("Path not trodden by this test")
            }
        }
        fn and(
            &self,
            _initial_witness: &mut BTreeMap<Witness, FieldElement>,
            _inputs: &[FunctionInput],
            _outputs: &[Witness],
        ) -> Result<OpcodeResolution, OpcodeResolutionError> {
            {
                panic!("Path not trodden by this test")
            }
        }
        fn xor(
            &self,
            _initial_witness: &mut BTreeMap<Witness, FieldElement>,
            _inputs: &[FunctionInput],
            _outputs: &[Witness],
        ) -> Result<OpcodeResolution, OpcodeResolutionError> {
            {
                panic!("Path not trodden by this test")
            }
        }
        fn range(
            &self,
            _initial_witness: &mut BTreeMap<Witness, FieldElement>,
            _inputs: &[FunctionInput],
            _outputs: &[Witness],
        ) -> Result<OpcodeResolution, OpcodeResolutionError> {
            {
                panic!("Path not trodden by this test")
            }
        }
        fn sha256(
            &self,
            _initial_witness: &mut BTreeMap<Witness, FieldElement>,
            _inputs: &[FunctionInput],
            _outputs: &[Witness],
        ) -> Result<OpcodeResolution, OpcodeResolutionError> {
            {
                panic!("Path not trodden by this test")
            }
        }
        fn blake2s(
            &self,
            _initial_witness: &mut BTreeMap<Witness, FieldElement>,
            _inputs: &[FunctionInput],
            _outputs: &[Witness],
        ) -> Result<OpcodeResolution, OpcodeResolutionError> {
            {
                panic!("Path not trodden by this test")
            }
        }
        fn compute_merkle_root(
            &self,
            _initial_witness: &mut BTreeMap<Witness, FieldElement>,
            _inputs: &[FunctionInput],
            _outputs: &[Witness],
        ) -> Result<OpcodeResolution, OpcodeResolutionError> {
            {
                panic!("Path not trodden by this test")
            }
        }
        fn schnorr_verify(
            &self,
            _initial_witness: &mut BTreeMap<Witness, FieldElement>,
            _inputs: &[FunctionInput],
            _outputs: &[Witness],
        ) -> Result<OpcodeResolution, OpcodeResolutionError> {
            {
                panic!("Path not trodden by this test")
            }
        }
        fn pedersen(
            &self,
            _initial_witness: &mut BTreeMap<Witness, FieldElement>,
            _inputs: &[FunctionInput],
            _outputs: &[Witness],
        ) -> Result<OpcodeResolution, OpcodeResolutionError> {
            {
                panic!("Path not trodden by this test")
            }
        }
        fn hash_to_field128_security(
            &self,
            _initial_witness: &mut BTreeMap<Witness, FieldElement>,
            _inputs: &[FunctionInput],
            _outputs: &[Witness],
        ) -> Result<OpcodeResolution, OpcodeResolutionError> {
            {
                panic!("Path not trodden by this test")
            }
        }
        fn ecdsa_secp256k1(
            &self,
            _initial_witness: &mut BTreeMap<Witness, FieldElement>,
            _inputs: &[FunctionInput],
            _outputs: &[Witness],
        ) -> Result<OpcodeResolution, OpcodeResolutionError> {
            {
                panic!("Path not trodden by this test")
            }
        }
        fn fixed_base_scalar_mul(
            &self,
            _initial_witness: &mut BTreeMap<Witness, FieldElement>,
            _inputs: &[FunctionInput],
            _outputs: &[Witness],
        ) -> Result<OpcodeResolution, OpcodeResolutionError> {
            {
                panic!("Path not trodden by this test")
            }
        }
        fn keccak256(
            &self,
            _initial_witness: &mut BTreeMap<Witness, FieldElement>,
            _inputs: &[FunctionInput],
            _outputs: &[Witness],
        ) -> Result<OpcodeResolution, OpcodeResolutionError> {
            {
                panic!("Path not trodden by this test")
            }
        }
    }

    #[test]
    fn inversion_oracle_equivalence() {
        // Opcodes below describe the following:
        // fn main(x : Field, y : pub Field) {
        //     let z = x + y;
        //     constrain 1/z == Oracle("inverse", x + y);
        // }
        let fe_0 = FieldElement::zero();
        let fe_1 = FieldElement::one();
        let w_x = Witness(1);
        let w_y = Witness(2);
        let w_oracle = Witness(3);
        let w_z = Witness(4);
        let w_z_inverse = Witness(5);
        let opcodes = vec![
            Opcode::Oracle(OracleData {
                name: "invert".into(),
                inputs: vec![Expression {
                    mul_terms: vec![],
                    linear_combinations: vec![(fe_1, w_x), (fe_1, w_y)],
                    q_c: fe_0,
                }],
                input_values: vec![],
                outputs: vec![w_oracle],
                output_values: vec![],
            }),
            Opcode::Arithmetic(Expression {
                mul_terms: vec![],
                linear_combinations: vec![(fe_1, w_x), (fe_1, w_y), (-fe_1, w_z)],
                q_c: fe_0,
            }),
            Opcode::Directive(Directive::Invert { x: w_z, result: w_z_inverse }),
            Opcode::Arithmetic(Expression {
                mul_terms: vec![(fe_1, w_z, w_z_inverse)],
                linear_combinations: vec![],
                q_c: -fe_1,
            }),
            Opcode::Arithmetic(Expression {
                mul_terms: vec![],
                linear_combinations: vec![(-fe_1, w_oracle), (fe_1, w_z_inverse)],
                q_c: fe_0,
            }),
        ];

        let backend = StubbedPwg;

        let mut witness_assignments = BTreeMap::from([
            (Witness(1), FieldElement::from(2u128)),
            (Witness(2), FieldElement::from(3u128)),
        ]);
        let mut blocks = Blocks::default();
        let solver_status = pwg::solve(&backend, &mut witness_assignments, &mut blocks, opcodes)
            .expect("should stall on oracle");
        let PartialWitnessGeneratorStatus::RequiresOracleData { mut required_oracle_data, unsolved_opcodes } = solver_status else {
            panic!("Should require oracle data")
        };
        assert!(unsolved_opcodes.is_empty(), "oracle should be removed");
        assert_eq!(required_oracle_data.len(), 1, "should have an oracle request");
        let mut oracle_data = required_oracle_data.remove(0);

        assert_eq!(oracle_data.input_values.len(), 1, "Should have solved a single input");

        // Filling data request and continue solving
        oracle_data.output_values = vec![oracle_data.input_values.last().unwrap().inverse()];
        let mut next_opcodes_for_solving = vec![Opcode::Oracle(oracle_data)];
        next_opcodes_for_solving.extend_from_slice(&unsolved_opcodes[..]);
        let solver_status =
            pwg::solve(&backend, &mut witness_assignments, &mut blocks, next_opcodes_for_solving)
                .expect("should be solvable");
        assert_eq!(solver_status, PartialWitnessGeneratorStatus::Solved, "should be fully solved");
    }
}
