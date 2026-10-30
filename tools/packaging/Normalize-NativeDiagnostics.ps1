param(
    [Parameter(Mandatory)][string]$Executable,
    [Parameter(Mandatory)][string]$PrivatePrefix,
    [Parameter(Mandatory)][string]$PublicPrefix
)
$ErrorActionPreference='Stop'
Add-Type -TypeDefinition @'
using System;
using System.IO;
using System.IO.MemoryMappedFiles;
using System.Runtime.InteropServices;
using System.Text;
using System.Text.RegularExpressions;

public static class NativeDiagnosticPaths {
    [DllImport("imagehlp.dll", SetLastError=true)]
    private static extern IntPtr CheckSumMappedFile(IntPtr address,uint length,out uint previous,out uint computed);

    private static void Require(bool valid,string message) {
        if(!valid) throw new InvalidDataException(message);
    }

    public static int Normalize(string path,string before,string after) {
        Require(before.Length != 0 && before.Length == after.Length,"Diagnostic prefixes must have equal UTF-16 length.");
        byte[] data=File.ReadAllBytes(path);
        Require(data.Length >= 64 && BitConverter.ToUInt16(data,0)==0x5a4d,"Not a PE image.");
        int pe=BitConverter.ToInt32(data,0x3c);
        Require(pe>=64 && pe<=data.Length-24 && BitConverter.ToUInt32(data,pe)==0x4550,"Invalid PE header.");
        int optional=checked(pe+24);
        int optionalSize=BitConverter.ToUInt16(data,pe+20);
        Require(optionalSize>=152 && optional<=data.Length-optionalSize && BitConverter.ToUInt16(data,optional)==0x20b,"Only PE32+ images are supported.");
        Require(BitConverter.ToUInt32(data,optional+144)==0 && BitConverter.ToUInt32(data,optional+148)==0,"Signed images are not modified.");
        int sections=BitConverter.ToUInt16(data,pe+6);
        int table=checked(optional+optionalSize);
        Require(table<=data.Length-checked(sections*40),"Truncated section table.");
        string[] prefixes={before,before.Replace("\\","\\\\")};
        string[] replacements={after,after.Replace("\\","\\\\")};
        int changes=0;
        for(int section=0;section<sections;section++) {
            int entry=checked(table+section*40);
            string name=Encoding.ASCII.GetString(data,entry,8).TrimEnd('\0');
            uint flags=BitConverter.ToUInt32(data,entry+36);
            if(name!=".rdata" || (flags&0x40000000)==0 || (flags&0xa0000000)!=0) continue;
            int size=checked((int)BitConverter.ToUInt32(data,entry+16));
            int start=checked((int)BitConverter.ToUInt32(data,entry+20));
            Require(start>=0 && size>=0 && start<=data.Length-size,"Invalid read-only data bounds.");
            char[] raw=new char[size/2];
            Buffer.BlockCopy(data,start,raw,0,raw.Length*2);
            string words=new string(raw);
            for(int variant=0;variant<prefixes.Length;variant++) {
                byte[] replacement=Encoding.Unicode.GetBytes(replacements[variant]);
                int offset=0;
                while((offset=words.IndexOf(prefixes[variant],offset,StringComparison.Ordinal))>=0) {
                    int end=words.IndexOf('\0',offset);
                    Require(end>=0 && end-offset<=32767,"Invalid native diagnostic string.");
                    int begin=words.LastIndexOf('\0',offset)+1;
                    string diagnostic=words.Substring(offset,end-offset);
                    string container=words.Substring(begin,end-begin);
                    bool filename=Regex.IsMatch(diagnostic,@"\.(?:c|cc|cpp|cxx|h|hh|hpp|hxx)$",RegexOptions.IgnoreCase|RegexOptions.CultureInvariant);
                    Require(filename || container.Contains("_wassert"),"The matched source prefix is not a native assertion diagnostic.");
                    Buffer.BlockCopy(replacement,0,data,checked(start+offset*2),replacement.Length);
                    changes++;
                    offset=checked(offset+prefixes[variant].Length);
                }
            }
        }
        if(changes==0) return 0;
        File.WriteAllBytes(path,data);
        using(var map=MemoryMappedFile.CreateFromFile(path,FileMode.Open,null,0,MemoryMappedFileAccess.ReadWrite))
        using(var view=map.CreateViewAccessor(0,0,MemoryMappedFileAccess.ReadWrite)) {
            uint previous,checksum;
            IntPtr result=CheckSumMappedFile(view.SafeMemoryMappedViewHandle.DangerousGetHandle(),checked((uint)data.Length),out previous,out checksum);
            Require(result!=IntPtr.Zero,"Could not compute the normalized image checksum.");
            view.Write(checked((long)optional+64),checksum);
            view.Flush();
        }
        return changes;
    }
}
'@
[NativeDiagnosticPaths]::Normalize([IO.Path]::GetFullPath($Executable),$PrivatePrefix,$PublicPrefix)
