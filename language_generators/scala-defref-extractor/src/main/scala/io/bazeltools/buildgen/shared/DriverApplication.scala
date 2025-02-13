package io.bazeltools.buildgen.shared

import cats.effect.{ExitCode, IO, IOApp}

import java.nio.file.Path
import com.monovore.decline.{Command, Opts}
import com.monovore.decline
import cats.implicits._
import io.circe.Encoder
import java.nio.file.Files
import java.nio.charset.StandardCharsets
import io.circe.syntax.EncoderOps

abstract class DriverApplication extends IOApp {
  def name: String
  def extract(data: String): IO[Symbols]

  def readToString(path: Path): IO[String] =
    IO.blocking(
      new String(Files.readAllBytes(path), StandardCharsets.UTF_8)
    )

  def writeJson[A: Encoder](path: Path, value: => A): IO[Unit] =
    IO.blocking {
      val _ = Files.write(
        path,
        value.asJson.noSpacesSortKeys.getBytes(StandardCharsets.UTF_8)
      )
    }

  private[this] def parallelExtractDataBlocks(
      workingDirectory: Path,
      paths: List[String]
  ): IO[List[DataBlock]] =
    paths.sorted
      .traverse { path =>
        // don't bother to parallelize reads, which are blocking operations
        // which could cause cats-effect to allocate a ton of IO bound
        // threads
        readToString(workingDirectory.resolve(path))
          .map((path, _))
      }
      .flatMap { inMemory =>
        // now that the data is in-memory,
        // in parallel using cpu-bound threadpool
        // which is limited to the number of CPUs
        // we can parse all the code
        inMemory.parTraverse { case (path, content) =>
          extract(content).attempt
            .flatMap {
              case Right(x) => IO.pure(x)
              case Left(err) =>
                IO.raiseError(
                  new Exception(
                    s"Failed in parsing of ${workingDirectory.resolve(path)}, with error\n$err",
                    err
                  )
                )
            }
            .map(_.withEntityPath(path))
        }
      }

  private[this] def sequentialExtractDataBlocks(
      workingDirectory: Path,
      paths: List[String]
  ): IO[List[DataBlock]] =
    paths.sorted
      .traverse { path =>
        val fullPath = workingDirectory.resolve(path)
        for {
          content <- readToString(fullPath)
          try_e <- extract(content).attempt
          symbols <- try_e match {
            case Right(x) => IO.pure(x)
            case Left(err) =>
              IO.raiseError(
                new Exception(
                  s"Failed in parsing of $fullPath, with error\n$err",
                  err
                )
              )
          }
        } yield symbols.withEntityPath(path)
      }

  private[this] def extractDataBlocks(
      parallel: Boolean,
      workingDirectory: Path,
      paths: List[String]
  ): IO[List[DataBlock]] =
    if (parallel) parallelExtractDataBlocks(workingDirectory, paths)
    else sequentialExtractDataBlocks(workingDirectory, paths)

  private[this] def isValidTld(s: String): Boolean =
    s.matches("^[a-z]+$")

  private[this] def setSpecialTlds(specialTldsEnv: String): IO[Unit] = {
    val names = specialTldsEnv.split(',').toList
    if (names.forall(isValidTld)) {
      IO(Entity.setSpecialTlds(names))
    } else {
      val invalid = names.filter(s => !isValidTld(s))
      IO.raiseError(new Exception(s"invalid TLDs: $invalid"))
    }
  }

  def getEnv(name: String): IO[Option[String]] =
    IO(Option(System.getenv(name)))

  def main: Command[IO[ExitCode]] = decline.Command(
    name,
    "Extract definitions and references from source files"
  ) {
    (
      Opts.option[String]("relative-input-paths", "input files to process"),
      Opts.option[Path]("working-directory", "input files to process"),
      Opts.option[String]("label-or-repo-path", "label to assign these files"),
      Opts.option[Path]("output", "target location to write to"),
      Opts
        .flag("sequential", "force parsing to be sequential to save memory")
        .orFalse
    ).mapN {
      (inputs, workingDirectory, label_or_repo_path, outputPath, sequential) =>
        for {
          specialTlds <- getEnv("BZL_GEN_SPECIAL_TLDS")
          _ <- setSpecialTlds(specialTlds.getOrElse(""))
          dataBlocks <- extractDataBlocks(
            parallel = !sequential,
            workingDirectory,
            inputs.split(',').toList
          )
          extractedData = ExtractedData(
            label_or_repo_path = label_or_repo_path,
            data_blocks = dataBlocks
          )
          _ <- writeJson(outputPath, extractedData)
        } yield ExitCode.Success
    }
  }

  def run(args: List[String]): IO[ExitCode] =
    IO(main.parse(args)).flatMap {
      case Right(exitCode) => exitCode
      case Left(err) =>
        IO {
          System.err.println(err.toString)
          ExitCode.Error
        }
    }
}
