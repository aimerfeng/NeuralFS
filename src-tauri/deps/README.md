# Runtime Dependencies

Place the following DLLs/libraries in this directory:

## ONNX Runtime
- `onnxruntime.dll` (Windows)
- `libonnxruntime.so` (Linux)
- `libonnxruntime.dylib` (macOS)

Download from: https://github.com/microsoft/onnxruntime/releases

## CUDA (Optional)
If using CUDA acceleration, ensure CUDA toolkit is installed:
- Windows: Install from NVIDIA website
- Linux: `apt install nvidia-cuda-toolkit` or similar

## Directory Structure
```
deps/
├── onnxruntime.dll
├── onnxruntime_providers_cuda.dll (optional)
├── onnxruntime_providers_shared.dll (optional)
└── README.md
```

## Environment Variables
- `ORT_LIB_LOCATION`: Path to ONNX Runtime library directory
- `CUDA_PATH`: Path to CUDA installation (Windows)
