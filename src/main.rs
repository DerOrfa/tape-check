use std::io::{BufRead, BufReader, ErrorKind, Write};
use md5::Context;
use std::path::{Path, PathBuf};
use tokio::{fs::File, io::AsyncReadExt, task::JoinSet};
use std::error::Error;
use std::process::Command;
use clap::{Parser, ValueHint::FilePath};

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// file(s) containing the md5 checksums
    #[arg(value_hint = FilePath, default_value="md5sum")]
    file:Vec<PathBuf>,
    /// maximum size of files active at the same time (in GBytes)
    #[arg(short,long,default_value_t=1024)]
    max_size:u64,
    ///release command
    #[arg(long)]
    release:Option<String>
}

async fn check_file(path:PathBuf, reference:String) -> std::io::Result<bool>
{
    // try open file until we get it, or it's a non-repeat-Error
    let mut res = File::open(&path).await;
    while let Err(err)= &res
    {
        match err.kind() {
            ErrorKind::TimedOut | ErrorKind::Interrupted => {res=File::open(&path).await}
            _ => {
                let desc=std::io::Error::other(format!("Failed to open {}",path.to_string_lossy()));
                return Err(std::io::Error::new(err.kind(),desc))
            }
        }
    };
    let mut file = res.expect("file should be valid here");
    let mut buffer=[0; 1024*512];
    let mut context = Context::new();
    while let Ok(size) = file.read(&mut buffer).await
    {
        context.write_all(&buffer[..size])?;
    }
    Ok(format!("{:x}", context.compute())==reference)
}
#[derive(Default)]
struct Reader
{
    readers:JoinSet<(PathBuf,std::io::Result<bool>)>,
    release:Vec<String>,
    cur_size:u64,max_size:u64
}

impl Reader
{
    fn new(max_size:u64, release:Option<String>)->Reader
    {
        let release= match release {
            None => vec![],
            Some(r) => {
                r.split_whitespace().map(String::from).collect()
            }
        };
        Reader{max_size,release,..Default::default()}
    }
    async fn add<T>(&mut self,path:T, reference:String) -> Result<(),Box<dyn Error>> where T:AsRef<Path>
    {
        let path = PathBuf::from(path.as_ref());
        let filesize = path.metadata()?.len();

        if filesize > self.max_size {
            return Err(format!(r#""{} is bigger than the maximum allowed buffer size {}"#,
                               path.to_string_lossy(),self.max_size).into());
        }

        // wait for files to finish until we're within our size allowance
        while self.cur_size + filesize > self.max_size {
            self.next().await?;
        }
        self.readers.spawn(async {
            (path.clone(),check_file(path,reference).await)
        });
        self.cur_size += filesize;
        Ok(())
    }
    async fn next(&mut self) -> Result<Option<(PathBuf,bool)>,Box<dyn Error>>
    {
        match self.readers.join_next().await.transpose()?
        {
            None => Ok(None),
            Some((path,Ok(ok))) =>
            {
                self.cur_size -= path.metadata()?.len();
                println!("{} {}",path.to_string_lossy(),if ok {"OK"} else {"FAIL"});
                self.release(&path);
                Ok(Some((path,ok)))
            }
            Some((path,Err(e))) => {
                self.release(&path);
                Err(format!(r#"failed reading {}: {e}"#,path.to_string_lossy()).into())
            }
        }
    }
    fn release<T>(&self,path:T) where T:AsRef<Path>
    {
        let (program,params)=self.release.split_first().unwrap();
        Command::new(program).args(params)
            .arg(path.as_ref().as_os_str())
            .status().ok();
    }
    async fn join(&mut self) -> Result<(),Box<dyn Error>>
    {
        while let Some(_) = self.next().await? {}
        Ok(())
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(),Box<dyn Error>>
{
    let args = Cli::parse();
    let mut reader = Reader::new(args.max_size^30,args.release);

    for md5filepath in args.file
    {
        let md5file = std::fs::File::open(md5filepath.as_path())
            .map_err(|e|format!("failed to open '{}': {e}",md5filepath.to_string_lossy()))?;
        let  md5base = md5filepath.parent().unwrap();//Should never be None, as File::open would have failed

        for line in BufReader::new(md5file).lines()
        {
            match line {
                Ok(line) => {
                    let (md5, filename) = line.split_at(32);
                    let filename = PathBuf::from(filename.trim());

                    reader.add(md5base.join(filename),md5.into()).await?;
                },
                Err(e) => { return Err(e.into()); }
            }
        }
    }
    reader.join().await
}
