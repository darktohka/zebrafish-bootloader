#include <efi.h>
#include <efilib.h>

#define KERNEL_PATH L"\\zebrafish-kernel"
#define CMDLINE_PATH L"\\cmdline.txt"
#define FALLBACK_CMDLINE L"initrd=\\zebrafish-initrd"

EFI_STATUS efi_main(EFI_HANDLE ImageHandle, EFI_SYSTEM_TABLE *SystemTable) {
    InitializeLib(ImageHandle, SystemTable);

    EFI_FILE_IO_INTERFACE *FileIO;
    EFI_FILE_HANDLE Volume;
    EFI_FILE_HANDLE File;
    EFI_LOADED_IMAGE *LoadedImage;
    EFI_STATUS Status;

    // Get Loaded Image Protocol
    Status = uefi_call_wrapper(BS->HandleProtocol, 3, ImageHandle, &LoadedImageProtocol, (void **)&LoadedImage);
    if (EFI_ERROR(Status)) {
        Print(L"Failed to get LoadedImageProtocol: %r\n", Status);
        return Status;
    }

    // Get FileSystem Protocol
    Status = uefi_call_wrapper(BS->HandleProtocol, 3, LoadedImage->DeviceHandle, &FileSystemProtocol, (void **)&FileIO);
    if (EFI_ERROR(Status)) {
        Print(L"Failed to get FileSystemProtocol: %r\n", Status);
        return Status;
    }

    // Open the volume
    Status = uefi_call_wrapper(FileIO->OpenVolume, 2, FileIO, &Volume);
    if (EFI_ERROR(Status)) {
        Print(L"Failed to open volume: %r\n", Status);
        return Status;
    }

    // Try to open \cmdline.txt
    CHAR16 CmdlineBuf[16384];
    UINTN CmdlineSize = sizeof(CmdlineBuf);
    Status = uefi_call_wrapper(Volume->Open, 5, Volume, &File, CMDLINE_PATH, EFI_FILE_MODE_READ, 0);
    if (!EFI_ERROR(Status)) {
        // Read file contents
        EFI_FILE_INFO *FileInfo;
        UINTN FileInfoSize = sizeof(EFI_FILE_INFO) + 256;
        FileInfo = AllocatePool(FileInfoSize);
        uefi_call_wrapper(File->GetInfo, 4, File, &GenericFileInfo, &FileInfoSize, FileInfo);

        if (FileInfo->FileSize >= CmdlineSize) FileInfo->FileSize = CmdlineSize - 2;
        CmdlineSize = FileInfo->FileSize;

        Status = uefi_call_wrapper(File->Read, 3, File, &CmdlineSize, CmdlineBuf);
        CmdlineBuf[CmdlineSize / sizeof(CHAR16)] = L'\0';
        uefi_call_wrapper(File->Close, 1, File);
    } else {
        StrCpy(CmdlineBuf, FALLBACK_CMDLINE);
    }

    // Load the kernel (EFI stub Linux kernel)
    EFI_HANDLE KernelImage;
    Status = uefi_call_wrapper(Volume->Open, 5, Volume, &File, KERNEL_PATH, EFI_FILE_MODE_READ, 0);
    if (EFI_ERROR(Status)) {
        Print(L"Kernel not found: %r\n", Status);
        return Status;
    }
    uefi_call_wrapper(File->Close, 1, File);

    EFI_DEVICE_PATH *DevicePath;
    Status = uefi_call_wrapper(BS->HandleProtocol, 3, LoadedImage->DeviceHandle, &DevicePathProtocol, (void **)&DevicePath);
    if (EFI_ERROR(Status)) {
        Print(L"Failed to get DevicePathProtocol: %r\n", Status);
        return Status;
    }

    EFI_DEVICE_PATH *KernelPathDP = FileDevicePath(LoadedImage->DeviceHandle, KERNEL_PATH);
    Status = uefi_call_wrapper(BS->LoadImage, 6, FALSE, ImageHandle, KernelPathDP, NULL, 0, &KernelImage);
    if (EFI_ERROR(Status)) {
        Print(L"LoadImage failed: %r\n", Status);
        return Status;
    }

    // Set command line arguments
    EFI_LOADED_IMAGE *KernelLoadedImage;
    Status = uefi_call_wrapper(BS->HandleProtocol, 3, KernelImage, &LoadedImageProtocol, (void **)&KernelLoadedImage);
    if (EFI_ERROR(Status)) {
        Print(L"Failed to get kernel loaded image protocol: %r\n", Status);
        return Status;
    }

    KernelLoadedImage->LoadOptions = CmdlineBuf;
    KernelLoadedImage->LoadOptionsSize = (StrLen(CmdlineBuf) + 1) * sizeof(CHAR16);

    // Start the kernel
    Status = uefi_call_wrapper(BS->StartImage, 3, KernelImage, NULL, NULL);
    Print(L"StartImage failed: %r\n", Status);
    return Status;
}
