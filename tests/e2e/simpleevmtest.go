package main

import (
	"context"
	"encoding/hex"
	"fmt"
	"strings"

	"google.golang.org/grpc"

	"github.com/oasisprotocol/oasis-core/go/common/cbor"
	"github.com/oasisprotocol/oasis-core/go/common/logging"

	"github.com/oasisprotocol/oasis-sdk/client-sdk/go/client"
	"github.com/oasisprotocol/oasis-sdk/client-sdk/go/crypto/signature"
	"github.com/oasisprotocol/oasis-sdk/client-sdk/go/testing"
	"github.com/oasisprotocol/oasis-sdk/client-sdk/go/types"

	"github.com/oasisprotocol/oasis-sdk/tests/e2e/txgen"
)

// The evmCreateTx type must match the CreateTx type from the evm module types
// in runtime-sdk/src/modules/evm/types.rs.
type evmCreateTx struct {
	Value    []byte `json:"value"`
	InitCode []byte `json:"init_code"`
	GasLimit uint64 `json:"gas_limit"`
}

// The evmCallTx type must match the CallTx type from the evm module types
// in runtime-sdk/src/modules/evm/types.rs.
type evmCallTx struct {
	Address  []byte `json:"address"`
	Value    []byte `json:"value"`
	Data     []byte `json:"data"`
	GasLimit uint64 `json:"gas_limit"`
}

// The evmPeekStorageQuery type must match the PeekStorageQuery type from the
// evm module types in runtime-sdk/src/modules/evm/types.rs.
type evmPeekStorageQuery struct {
	Address []byte `json:"address"`
	Index   []byte `json:"index"`
}

// The evmPeekCodeQuery type must match the PeekCodeQuery type from the
// evm module types in runtime-sdk/src/modules/evm/types.rs.
type evmPeekCodeQuery struct {
	Address []byte `json:"address"`
}

func evmCreate(ctx context.Context, rtc client.RuntimeClient, signer signature.Signer, tx evmCreateTx) ([]byte, error) {
	rawTx := types.NewTransaction(nil, "evm.Create", tx)
	result, err := txgen.SignAndSubmitTx(ctx, rtc, signer, *rawTx)
	if err != nil {
		return nil, err
	}
	var out []byte
	if err = cbor.Unmarshal(result, &out); err != nil {
		return nil, fmt.Errorf("failed to unmarshal evmCreate result: %w", err)
	}
	return out, nil
}

func evmCall(ctx context.Context, rtc client.RuntimeClient, signer signature.Signer, tx evmCallTx) ([]byte, error) {
	rawTx := types.NewTransaction(nil, "evm.Call", tx)
	result, err := txgen.SignAndSubmitTx(ctx, rtc, signer, *rawTx)
	if err != nil {
		return nil, err
	}
	var out []byte
	if err = cbor.Unmarshal(result, &out); err != nil {
		return nil, fmt.Errorf("failed to unmarshal evmCall result: %w", err)
	}
	return out, nil
}

func evmPeekStorage(ctx context.Context, rtc client.RuntimeClient, q evmPeekStorageQuery) ([]byte, error) {
	var res []byte
	if err := rtc.Query(ctx, client.RoundLatest, "evm.PeekStorage", q, &res); err != nil {
		return nil, err
	}
	return res, nil
}

func evmPeekCode(ctx context.Context, rtc client.RuntimeClient, q evmPeekCodeQuery) ([]byte, error) {
	var res []byte
	if err := rtc.Query(ctx, client.RoundLatest, "evm.PeekCode", q, &res); err != nil {
		return nil, err
	}
	return res, nil
}

// This wraps the given EVM bytecode in an unpacker, suitable for
// passing as the init code to evmCreate.
func evmPack(bytecode []byte) []byte {
	var need16bits bool
	if len(bytecode) > 255 {
		need16bits = true
	}
	if len(bytecode) > 65535 {
		// It's unlikely we'll need anything bigger than this in tests.
		panic("bytecode too long (must be under 64kB)")
	}

	var lenFmt string
	var push string
	var offTag string
	if need16bits {
		lenFmt = "%04x"
		push = "61" // PUSH2.
		offTag = "XXXX"
	} else {
		lenFmt = "%02x"
		push = "60" // PUSH1.
		offTag = "XX"
	}

	bcLen := fmt.Sprintf(lenFmt, len(bytecode))

	// The EVM expects the init code that's passed to CREATE to copy the
	// actual contract's bytecode into temporary memory and return it.
	// The EVM then stores it into code storage at the contract's address.

	var unpacker string
	unpacker += push   // PUSH1 or PUSH2.
	unpacker += bcLen  // Number of bytes in contract.
	unpacker += push   // PUSH1 or PUSH2.
	unpacker += offTag // Offset of code payload in this bytecode (calculated below).
	unpacker += "60"   // PUSH1.
	unpacker += "00"   // Where to put the code in memory.
	unpacker += "39"   // CODECOPY -- copy code into memory.
	unpacker += push   // PUSH1 or PUSH2.
	unpacker += bcLen  // Number of bytes in contract.
	unpacker += "60"   // PUSH1.
	unpacker += "00"   // Where the code is in memory.
	unpacker += "f3"   // RETURN.

	// Patch the offset.
	offset := fmt.Sprintf(lenFmt, len(unpacker)/2)
	finalBytecodeSrc := strings.ReplaceAll(unpacker, offTag, offset)

	// Convert to bytes.
	packedBytecode, err := hex.DecodeString(finalBytecodeSrc)
	if err != nil {
		panic("can't decode hex")
	}

	// Append the actual contract's bytecode to the end of the unpacker.
	packedBytecode = append(packedBytecode, bytecode...)

	return packedBytecode
}

// SimpleEVMTest does a simple EVM test.
func SimpleEVMTest(sc *RuntimeScenario, log *logging.Logger, conn *grpc.ClientConn, rtc client.RuntimeClient) error {
	ctx := context.Background()
	signer := testing.Dave.Signer

	value, err := hex.DecodeString(strings.Repeat("0", 64))
	if err != nil {
		return err
	}

	// Create a simple contract that adds two numbers and stores the result
	// in slot 0 of its storage.
	var addSrc string
	addSrc += "60" // PUSH1.
	addSrc += "12" // Constant 0x12.
	addSrc += "60" // PUSH1.
	addSrc += "34" // Constant 0x34.
	addSrc += "01" // ADD.
	addSrc += "60" // PUSH1.
	addSrc += "00" // Constant 0.
	addSrc += "55" // SSTORE 00<-46.

	addBytecode, err := hex.DecodeString(addSrc)
	if err != nil {
		return err
	}
	addPackedBytecode := evmPack(addBytecode)

	// Create the EVM contract.
	contractAddr, err := evmCreate(ctx, rtc, signer, evmCreateTx{
		Value:    value,
		InitCode: addPackedBytecode,
		GasLimit: 64000,
	})
	if err != nil {
		return fmt.Errorf("evmCreate failed: %w", err)
	}

	log.Info("evmCreate finished", "contract_addr", hex.EncodeToString(contractAddr))

	// Peek into code storage to verify that our contract was indeed stored.
	storedCode, err := evmPeekCode(ctx, rtc, evmPeekCodeQuery{
		Address: contractAddr,
	})
	if err != nil {
		return fmt.Errorf("evmPeekCode failed: %w", err)
	}

	storedCodeHex := hex.EncodeToString(storedCode)
	log.Info("evmPeekCode finished", "stored_code", storedCodeHex)

	if storedCodeHex != addSrc {
		return fmt.Errorf("stored code doesn't match original code")
	}

	// Call the created EVM contract.
	callResult, err := evmCall(ctx, rtc, signer, evmCallTx{
		Address:  contractAddr,
		Value:    value,
		Data:     []byte{},
		GasLimit: 64000,
	})
	if err != nil {
		return fmt.Errorf("evmCall failed: %w", err)
	}

	log.Info("evmCall finished", "call_result", hex.EncodeToString(callResult))

	// Peek at the EVM storage to get the final result we stored there.
	index, err := hex.DecodeString(strings.Repeat("0", 64))
	if err != nil {
		return err
	}

	storedVal, err := evmPeekStorage(ctx, rtc, evmPeekStorageQuery{
		Address: contractAddr,
		Index:   index,
	})
	if err != nil {
		return fmt.Errorf("evmPeekStorage failed: %w", err)
	}

	storedValHex := hex.EncodeToString(storedVal)
	log.Info("evmPeekStorage finished", "stored_value", storedValHex)

	if storedValHex != strings.Repeat("0", 62)+"46" {
		return fmt.Errorf("stored value isn't correct (expected 0x46)")
	}

	return nil
}

// SimpleSolEVMTest does a simple Solidity contract test.
func SimpleSolEVMTest(sc *RuntimeScenario, log *logging.Logger, conn *grpc.ClientConn, rtc client.RuntimeClient) error {
	ctx := context.Background()
	signer := testing.Dave.Signer

	// To generate the contract bytecode below, use https://remix.ethereum.org/
	// with the following settings:
	//     Compiler: 0.8.6+commit.11564f7e
	//     EVM version: istanbul
	//     Enable optimization: yes, 200
	// on the following source:
	/*
		pragma solidity ^0.8.0;

		contract Foo {
			constructor() public {}

			function name() public view returns (string memory) {
				return "test";
			}
		}
	*/

	contract, err := hex.DecodeString("608060405234801561001057600080fd5b5060e28061001f6000396000f3fe6080604052348015600f57600080fd5b506004361060285760003560e01c806306fdde0314602d575b600080fd5b60408051808201825260048152631d195cdd60e21b6020820152905160519190605a565b60405180910390f35b600060208083528351808285015260005b81811015608557858101830151858201604001528201606b565b818111156096576000604083870101525b50601f01601f191692909201604001939250505056fea26469706673582212208bdadb079b568a734c06b694ff7b4b03ad5fcb911f0d86fe0519e6ed5bfb3fd764736f6c63430008060033")
	if err != nil {
		return err
	}

	zero, err := hex.DecodeString(strings.Repeat("0", 64))
	if err != nil {
		return err
	}

	// Create the EVM contract.
	contractAddr, err := evmCreate(ctx, rtc, signer, evmCreateTx{
		Value:    zero,
		InitCode: contract,
		GasLimit: 128000,
	})
	if err != nil {
		return fmt.Errorf("evmCreate failed: %w", err)
	}

	log.Info("evmCreate finished", "contract_addr", hex.EncodeToString(contractAddr))

	// This is the hash of the "name()" method of the contract.
	// You can get this by clicking on "Compilation details" and then
	// looking at the "Function hashes" section.
	// Method calls must be zero-padded to a multiple of 32 bytes.
	nameMethod, err := hex.DecodeString("06fdde03" + strings.Repeat("0", 64-8))
	if err != nil {
		return err
	}

	// Call the name method.
	callResult, err := evmCall(ctx, rtc, signer, evmCallTx{
		Address:  contractAddr,
		Value:    zero,
		Data:     nameMethod,
		GasLimit: 22000,
	})
	if err != nil {
		return fmt.Errorf("evmCall failed: %w", err)
	}

	res := hex.EncodeToString(callResult)
	log.Info("evmCall:name finished", "call_result", res)

	if len(res) != 192 {
		return fmt.Errorf("returned value has wrong length (expected 192, got %d)", len(res))
	}
	if res[127:136] != "474657374" {
		// The returned string is packed as length (4) + "test" in hex.
		return fmt.Errorf("returned value is incorrect (expected '474657374', got '%s')", res[127:136])
	}

	return nil
}

// SimpleERC20EVMTest does a simple ERC20 contract test.
func SimpleERC20EVMTest(sc *RuntimeScenario, log *logging.Logger, conn *grpc.ClientConn, rtc client.RuntimeClient) error {
	ctx := context.Background()
	signer := testing.Dave.Signer

	// To generate the contract bytecode below, use https://remix.ethereum.org/
	// with the following settings:
	//     Compiler: 0.8.6+commit.11564f7e
	//     EVM version: istanbul
	//     Enable optimization: yes, 200
	// on the following source:
	/*
		pragma solidity ^0.8.0;
		import "@openzeppelin/contracts/token/ERC20/ERC20.sol";

		contract TestToken is ERC20 {
			constructor() ERC20("Test", "TST") public {
				_mint(msg.sender, 1000000 * (10 ** uint256(decimals())));
			}
		}
	*/

	erc20, err := hex.DecodeString("60806040523480156200001157600080fd5b506040518060400160405280600481526020016315195cdd60e21b815250604051806040016040528060038152602001621514d560ea1b815250816003908051906020019062000063929190620001a9565b50805162000079906004906020840190620001a9565b505050620000b63362000091620000bc60201b60201c565b620000a19060ff16600a620002b3565b620000b090620f42406200037e565b620000c1565b620003f3565b601290565b6001600160a01b0382166200011c5760405162461bcd60e51b815260206004820152601f60248201527f45524332303a206d696e7420746f20746865207a65726f206164647265737300604482015260640160405180910390fd5b80600260008282546200013091906200024f565b90915550506001600160a01b038216600090815260208190526040812080548392906200015f9084906200024f565b90915550506040518181526001600160a01b038316906000907fddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef9060200160405180910390a35050565b828054620001b790620003a0565b90600052602060002090601f016020900481019282620001db576000855562000226565b82601f10620001f657805160ff191683800117855562000226565b8280016001018555821562000226579182015b828111156200022657825182559160200191906001019062000209565b506200023492915062000238565b5090565b5b8082111562000234576000815560010162000239565b60008219821115620002655762000265620003dd565b500190565b600181815b80851115620002ab5781600019048211156200028f576200028f620003dd565b808516156200029d57918102915b93841c93908002906200026f565b509250929050565b6000620002c18383620002c8565b9392505050565b600082620002d95750600162000378565b81620002e85750600062000378565b81600181146200030157600281146200030c576200032c565b600191505062000378565b60ff841115620003205762000320620003dd565b50506001821b62000378565b5060208310610133831016604e8410600b841016171562000351575081810a62000378565b6200035d83836200026a565b8060001904821115620003745762000374620003dd565b0290505b92915050565b60008160001904831182151516156200039b576200039b620003dd565b500290565b600181811c90821680620003b557607f821691505b60208210811415620003d757634e487b7160e01b600052602260045260246000fd5b50919050565b634e487b7160e01b600052601160045260246000fd5b6108c480620004036000396000f3fe608060405234801561001057600080fd5b50600436106100a95760003560e01c80633950935111610071578063395093511461012357806370a082311461013657806395d89b411461015f578063a457c2d714610167578063a9059cbb1461017a578063dd62ed3e1461018d57600080fd5b806306fdde03146100ae578063095ea7b3146100cc57806318160ddd146100ef57806323b872dd14610101578063313ce56714610114575b600080fd5b6100b66101c6565b6040516100c391906107d8565b60405180910390f35b6100df6100da3660046107ae565b610258565b60405190151581526020016100c3565b6002545b6040519081526020016100c3565b6100df61010f366004610772565b61026e565b604051601281526020016100c3565b6100df6101313660046107ae565b61031d565b6100f361014436600461071d565b6001600160a01b031660009081526020819052604090205490565b6100b6610359565b6100df6101753660046107ae565b610368565b6100df6101883660046107ae565b610401565b6100f361019b36600461073f565b6001600160a01b03918216600090815260016020908152604080832093909416825291909152205490565b6060600380546101d590610853565b80601f016020809104026020016040519081016040528092919081815260200182805461020190610853565b801561024e5780601f106102235761010080835404028352916020019161024e565b820191906000526020600020905b81548152906001019060200180831161023157829003601f168201915b5050505050905090565b600061026533848461040e565b50600192915050565b600061027b848484610532565b6001600160a01b0384166000908152600160209081526040808320338452909152902054828110156103055760405162461bcd60e51b815260206004820152602860248201527f45524332303a207472616e7366657220616d6f756e74206578636565647320616044820152676c6c6f77616e636560c01b60648201526084015b60405180910390fd5b610312853385840361040e565b506001949350505050565b3360008181526001602090815260408083206001600160a01b0387168452909152812054909161026591859061035490869061082d565b61040e565b6060600480546101d590610853565b3360009081526001602090815260408083206001600160a01b0386168452909152812054828110156103ea5760405162461bcd60e51b815260206004820152602560248201527f45524332303a2064656372656173656420616c6c6f77616e63652062656c6f77604482015264207a65726f60d81b60648201526084016102fc565b6103f7338585840361040e565b5060019392505050565b6000610265338484610532565b6001600160a01b0383166104705760405162461bcd60e51b8152602060048201526024808201527f45524332303a20617070726f76652066726f6d20746865207a65726f206164646044820152637265737360e01b60648201526084016102fc565b6001600160a01b0382166104d15760405162461bcd60e51b815260206004820152602260248201527f45524332303a20617070726f766520746f20746865207a65726f206164647265604482015261737360f01b60648201526084016102fc565b6001600160a01b0383811660008181526001602090815260408083209487168084529482529182902085905590518481527f8c5be1e5ebec7d5bd14f71427d1e84f3dd0314c0f7b2291e5b200ac8c7c3b925910160405180910390a3505050565b6001600160a01b0383166105965760405162461bcd60e51b815260206004820152602560248201527f45524332303a207472616e736665722066726f6d20746865207a65726f206164604482015264647265737360d81b60648201526084016102fc565b6001600160a01b0382166105f85760405162461bcd60e51b815260206004820152602360248201527f45524332303a207472616e7366657220746f20746865207a65726f206164647260448201526265737360e81b60648201526084016102fc565b6001600160a01b038316600090815260208190526040902054818110156106705760405162461bcd60e51b815260206004820152602660248201527f45524332303a207472616e7366657220616d6f756e7420657863656564732062604482015265616c616e636560d01b60648201526084016102fc565b6001600160a01b038085166000908152602081905260408082208585039055918516815290812080548492906106a790849061082d565b92505081905550826001600160a01b0316846001600160a01b03167fddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef846040516106f391815260200190565b60405180910390a350505050565b80356001600160a01b038116811461071857600080fd5b919050565b60006020828403121561072f57600080fd5b61073882610701565b9392505050565b6000806040838503121561075257600080fd5b61075b83610701565b915061076960208401610701565b90509250929050565b60008060006060848603121561078757600080fd5b61079084610701565b925061079e60208501610701565b9150604084013590509250925092565b600080604083850312156107c157600080fd5b6107ca83610701565b946020939093013593505050565b600060208083528351808285015260005b81811015610805578581018301518582016040015282016107e9565b81811115610817576000604083870101525b50601f01601f1916929092016040019392505050565b6000821982111561084e57634e487b7160e01b600052601160045260246000fd5b500190565b600181811c9082168061086757607f821691505b6020821081141561088857634e487b7160e01b600052602260045260246000fd5b5091905056fea264697066735822122057fae6e23c9b696979cb61373ad6bb8f5e6f3dd858b98a3b12e629cd6536fa5764736f6c63430008060033")
	if err != nil {
		return err
	}

	zero, err := hex.DecodeString(strings.Repeat("0", 64))
	if err != nil {
		return err
	}

	// Create the EVM contract.
	contractAddr, err := evmCreate(ctx, rtc, signer, evmCreateTx{
		Value:    zero,
		InitCode: erc20,
		GasLimit: 1024000,
	})
	if err != nil {
		return fmt.Errorf("evmCreate failed: %w", err)
	}

	log.Info("evmCreate finished", "contract_addr", hex.EncodeToString(contractAddr))

	// This is the hash of the "name()" method of the contract.
	// You can get this by clicking on "Compilation details" and then
	// looking at the "Function hashes" section.
	// Method calls must be zero-padded to a multiple of 32 bytes.
	nameMethod, err := hex.DecodeString("06fdde03" + strings.Repeat("0", 64-8))
	if err != nil {
		return err
	}

	// Call the name method.
	callResult, err := evmCall(ctx, rtc, signer, evmCallTx{
		Address:  contractAddr,
		Value:    zero,
		Data:     nameMethod,
		GasLimit: 25000,
	})
	if err != nil {
		return fmt.Errorf("evmCall:name failed: %w", err)
	}

	resName := hex.EncodeToString(callResult)
	log.Info("evmCall:name finished", "call_result", resName)

	if len(resName) != 192 {
		return fmt.Errorf("returned value has wrong length (expected 192, got %d)", len(resName))
	}
	if resName[127:136] != "454657374" {
		// The returned string is packed as length (4) + "Test" in hex.
		return fmt.Errorf("returned value is incorrect (expected '454657374', got '%s')", resName[127:136])
	}

	// Call transfer(0x123, 0x42).
	transferMethod, err := hex.DecodeString("a9059cbb" + strings.Repeat("0", 64-3) + "123" + strings.Repeat("0", 64-2) + "42")
	if err != nil {
		return err
	}
	callResult, err = evmCall(ctx, rtc, signer, evmCallTx{
		Address:  contractAddr,
		Value:    zero,
		Data:     transferMethod,
		GasLimit: 64000,
	})
	if err != nil {
		return fmt.Errorf("evmCall:transfer failed: %w", err)
	}

	resTransfer := hex.EncodeToString(callResult)
	log.Info("evmCall:transfer finished", "call_result", resTransfer)

	// Return value should be true.
	if resTransfer != strings.Repeat("0", 64-1)+"1" {
		return fmt.Errorf("return value of transfer method call should be true")
	}

	// Call balanceOf(0x123).
	balanceMethod, err := hex.DecodeString("70a08231" + strings.Repeat("0", 64-3) + "123")
	if err != nil {
		return err
	}
	callResult, err = evmCall(ctx, rtc, signer, evmCallTx{
		Address:  contractAddr,
		Value:    zero,
		Data:     balanceMethod,
		GasLimit: 32000,
	})
	if err != nil {
		return fmt.Errorf("evmCall:balanceOf failed: %w", err)
	}

	resBalance := hex.EncodeToString(callResult)
	log.Info("evmCall:balanceOf finished", "call_result", resBalance)

	// Balance should match the amount we transferred.
	if resBalance != strings.Repeat("0", 64-2)+"42" {
		return fmt.Errorf("return value of balanceOf method call should be 0x42")
	}

	return nil
}
