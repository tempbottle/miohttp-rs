use std::fmt;
use std::path::Path;

pub enum Type {
    TextHtml,
    TextPlain,
    ImageJpeg,
    ImagePng,
}

impl Type {
    pub fn to_str(&self) -> &str {
        match *self {
            Type::TextHtml => "text/html; charset=utf-8",
            Type::TextPlain => "text/plain",
            Type::ImageJpeg => "image/jpeg",
            Type::ImagePng => "image/png",
        }
    }


    pub fn create_from_path(path: &Path) -> Type {
        
        // TODO: Match on strings is slow, maybe some b-tree?
        
        match path.extension() {
            
            Some(ext) => match ext.to_str() {
                Some("txt")  => Type::TextPlain,
                Some("jpg")  => Type::ImageJpeg,
                Some("png")  => Type::ImagePng,
                //Some("html") => Type::TextHtml,
                Some(_)      => Type::TextHtml,
                None         => Type::TextHtml,
            },
            
            None => Type::TextHtml,
        }
    }

}

impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.to_str())
    }
}
